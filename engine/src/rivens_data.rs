//! Riven mods: the stat pool from `data/rivens/<class>.yaml`, the value
//! formula, the generated name, and the [`ModDef`] a riven resolves to.
//!
//! A riven is not a mod with fixed numbers — it is a mod whose numbers are
//! CONSTRUCTED from a roll, and this is that construction.
//!
//! ```text
//! shown value = base x 10 x (rank + 1) x disposition x config x roll
//! ```
//!
//! `base` is DE's own per-stat number (`upgradeEntries` in the export). The
//! `10 x (rank + 1)` term is 90 at rank 8, and TWO independent sources agree on
//! it: the wiki's own base-value column IS DE's number times 90, to four
//! figures including the ugly ones — 164.9997 / 149.9940 / 60.0300 against
//! Damage 165%, Critical Chance 149.99%, Fire Rate 60.03%.
//!
//! Every stat rolls its 0.9-1.1 INDEPENDENTLY, with no shared per-riven
//! quality, so the corner where every bonus is maximal and the malus minimal is
//! legal and astronomically unlikely. This is a CONSTRUCTOR rather than a
//! roller and must reach that corner: it is the ceiling the optimizer wants.

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
    /// Multiplier on every POSITIVE stat — the wiki's table, which is the only
    /// published set verified from a primary source AND internally consistent
    /// (1.2375 is exactly `0.99 x 1.25`, 0.9375 exactly `0.75 x 1.25`). The
    /// three tables that disagree, and why this one wins, are
    /// docs/DATA_SOURCES.md §"Riven config multiplier".
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
    /// where the stored value is -0.41. Only the three
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
    /// riven they own. A percentage shows ONE, so a box
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
/// a judgement about it.
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

/// Riven stats THIS WEAPON cannot roll, out of its class pool.
///
/// The pool is per CLASS, but two rifles do not roll the same stats: DE does
/// not hand a weapon an attribute for a stat it does not have. What generates
/// a pool is docs/DATA_SOURCES.md §"Riven pools", and THE DERIVATION IS THE
/// LAST OF THREE SOURCES — `data/rivens/exceptions.yaml` overrides per riven
/// FAMILY, and these rules fill in for a family nobody has a card from.
/// `data/rivens/pools.yaml` is neither: a count over live listings, read by
/// `the_survey_still_agrees_with_the_rules` and by nothing in the calculation.
///
/// 1. **Physical damage** — *"Weapons without more than 25% of a physical
///    damage type usually cannot roll that respective attribute … Exceptions
///    exist on a case by case basis."* That clause is why this is a derivation
///    and not a law.
/// 2. **A stat the weapon does not have** is inert whatever DE rolls, and the
///    weapon's own wiki table is the evidence — Verglas Prime has no Zoom row,
///    no Recoil row, "Ammo Max: ∞" and "Projectile Type: Hit-Scan".
///
/// Both rules read the weapon as ONE THING WITH FORMS, over the union of the
/// forms you can fire for free: Larkspur Prime's beam is 11% Impact and its
/// alt-fire 33%, and a real card rolls negative Impact. A GAUGE-SWITCHED form
/// stays out.
pub fn derived_for(weapon_id: &str) -> Vec<&'static str> {
    let Some(s) = crate::weapons_data::spec(weapon_id) else { return Vec::new() };
    // A RIVEN BELONGS TO A FAMILY, so the pool is the family's. One card equips on every member — a Ballistica riven is a
    // Ballistica Prime riven and a Rakta Ballistica riven — so a pool derived
    // from ONE member describes a card that does not exist. Fifteen families
    // disagreed with themselves before this: a real card carrying negative
    // Slash rolls legally on the Ballistica Prime (18% Slash on its charged
    // shot) and was refused as "not a legal riven" on the other two.
    //
    // The members, then each member's own free FORMS, and the shot rules read
    // the union of all of it — which is the same argument the alt-fire rule
    // already makes, one level up.
    let family: Vec<&'static crate::weapons_data::WeaponSpec> = match &s.riven_family {
        Some(f) => crate::weapons_data::all()
            .iter()
            .filter(|w| w.riven_family.as_deref() == Some(f.as_str()))
            .collect(),
        // A weapon with no family answers for itself, which is what it means.
        None => vec![s],
    };
    // `s` is the entry the caller named — what the rules that read the WEAPON
    // (its ammo pool, its class) go to. `forms` is what the rules that read a
    // SHOT go to, and there can be more than one of those, on more than one
    // member.
    let forms: Vec<_> = family
        .iter()
        .flat_map(|w| crate::weapons_data::forms_of(&w.id))
        .filter(|f| !f.kind.is_adapter_form())
        .filter_map(|f| crate::weapons_data::spec(f.weapon_id))
        .collect();
    let mut out: Vec<&'static str> = Vec::new();

    for (stat, key) in [("impact", "impact"), ("puncture", "puncture"), ("slash", "slash")] {
        let best = forms
            .iter()
            .map(|f| {
                let total: f64 = f.attack.damage.values().sum();
                if total > 0.0 {
                    f.attack.damage.get(key).copied().unwrap_or(0.0) / total
                } else {
                    0.0
                }
            })
            .fold(0.0_f64, f64::max);
        if best <= 0.25 {
            out.push(stat);
        }
    }
    // The player never aims a sentinel weapon, so it has neither stat. Same
    // `class.contains("sentinel")` test the exilus and arcane rules use.
    //
    // ASKED OF EVERY MEMBER, like the shot rules above: a stat is inert only if
    // it is inert on the whole family, because one card covers the whole family.
    if family.iter().all(|w| w.class.contains("sentinel")) {
        out.push("zoom");
        out.push("weapon_recoil");
    }
    // No ammo pool at all — a percentage of infinity is not a stat.
    if family.iter().all(|w| w.ammo_max.is_none()) {
        out.push("ammo_maximum");
    }
    // NOTHING FOR FLIGHT SPEED TO ACT ON — and there are TWO ways to give it
    // something. Wiki (`Projectile Speed`), verbatim: *"Mods including Rivens
    // that have positive or negative Projectile speeds will affect a weapon's
    // entire Damage Falloff range accordingly"*, and *"Hitscan weapons that do
    // **not** list Damage Falloff values in their UI are completely unaffected
    // by Projectile Speed modifications"*.
    //
    // So a falloff counts even with nothing in the air, which is why a shotgun
    // rolls the stat: the Boar keeps 50% past 25 m and the riven moves that
    // whole range. Reading only `shot_type` said no to every shotgun in the
    // roster.
    let flies = |f: &&'static crate::weapons_data::WeaponSpec| {
        f.attack.shot_type.is_some_and(|t| t.flies()) || f.attack.falloff.is_some()
    };
    if !forms.iter().any(flies) {
        out.push("projectile_speed");
    }

    out
}

/// What this weapon's rivens can NOT roll: the derivation, with the
/// hand-written per-family EXCEPTIONS applied over it.
///
/// TWO LAYERS AND NOT THREE, and which two is the point.
/// The rules generate the pool; `data/rivens/exceptions.yaml` overrides them
/// where somebody has looked; the SURVEY is neither of those and no longer
/// appears here at all.
///
/// It must not. `pools.yaml` outranking the derivation makes a scrape a silent
/// authority over 26 weapon families, and a re-run of one came back "nothing
/// rolls anything" for every one of them. Nothing in the pipeline catches that:
/// the file parses, the pools empty, and the two
/// tests that failed were both about something else. Evidence belongs in a
/// check (`the_survey_still_agrees_with_the_rules`), where a broken scrape is
/// loud.
///
/// The exception list speaks per riven FAMILY, because that is the unit DE
/// rolls: one Boar riven fits the Boar and the Boar Prime, so one entry covers
/// both.
pub fn excluded_for(weapon_id: &str) -> Vec<&'static str> {
    let Some(s) = crate::weapons_data::spec(weapon_id) else { return Vec::new() };
    let mut out = derived_for(weapon_id);
    let Some(fam) = s.riven_family.as_deref() else { return out };
    let ex = exceptions(fam);
    out.retain(|id| !ex.rolls.contains(id));
    for id in &ex.never {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

/// DE's OWN riven family names — `data/rivens/de_families.yaml`, written by
/// `scripts/survey_riven_families.py` from the weekly trade dump's
/// `compatibility` field.
///
/// It is ONE WEEK of trades, so it CONFIRMS a name and can never refute one: a
/// family absent from it is a family nobody traded that week.
pub fn de_families() -> &'static [String] {
    use std::sync::OnceLock;
    static F: OnceLock<Vec<String>> = OnceLock::new();
    F.get_or_init(|| {
        #[derive(Deserialize)]
        struct File {
            families: Vec<String>,
        }
        let (p, text) = crate::data::files_under("rivens/")
            .find(|(p, _)| p.ends_with("de_families.yaml"))
            .expect("data/rivens/de_families.yaml");
        let f: File = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
        f.families
    })
}

/// One riven family's surveyed pool — `data/rivens/pools.yaml`.
pub struct SurveyedPool {
    pub family: String,
    /// How many listings the count is over.
    pub n: u32,
    pub rollable: Vec<&'static str>,
    pub never: Vec<&'static str>,
}

#[derive(Deserialize)]
struct PoolsFile {
    #[allow(dead_code)]
    surveyed: String,
    families: Vec<RawSurvey>,
}

#[derive(Deserialize)]
struct RawSurvey {
    family: String,
    n: u32,
    #[serde(default)]
    rollable: Vec<String>,
    #[serde(default)]
    never: Vec<String>,
}

/// One family's hand-written exceptions to the derivation.
#[derive(Default)]
pub struct Exceptions {
    /// The rules refuse it and it is real.
    pub rolls: Vec<&'static str>,
    /// The rules allow it and it is not.
    pub never: Vec<&'static str>,
}

#[derive(Deserialize)]
struct ExceptionsFile {
    families: Vec<RawExceptions>,
}

#[derive(Deserialize)]
struct RawExceptions {
    family: String,
    #[serde(default)]
    rolls: Vec<ExceptionStat>,
    #[serde(default)]
    never: Vec<ExceptionStat>,
}

#[derive(Deserialize)]
struct ExceptionStat {
    stat: String,
    /// What was looked at. REQUIRED — the note IS the evidence, and serde
    /// refuses the entry without one.
    #[allow(dead_code)]
    note: String,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every surveyed family, loaded once.
pub fn surveys() -> &'static [SurveyedPool] {
    static S: OnceLock<Vec<SurveyedPool>> = OnceLock::new();
    S.get_or_init(|| {
        crate::data::files_under("rivens/")
            .filter(|(p, _)| *p == "rivens/pools.yaml")
            .filter_map(|(_, t)| serde_norway::from_str::<PoolsFile>(t).ok())
            .flat_map(|f| f.families)
            .map(|r| SurveyedPool {
                family: r.family,
                n: r.n,
                rollable: r.rollable.into_iter().map(leak).collect(),
                never: r.never.into_iter().map(leak).collect(),
            })
            .collect()
    })
}

/// One family's surveyed pool. VERIFICATION ONLY — nothing in the calculation
/// reads this; `the_survey_still_agrees_with_the_rules` does.
pub fn survey(family: &str) -> Option<&'static SurveyedPool> {
    surveys().iter().find(|s| s.family == family)
}

/// One family's exceptions — `data/rivens/exceptions.yaml`.
pub fn exceptions(family: &str) -> &'static Exceptions {
    static S: OnceLock<std::collections::BTreeMap<String, Exceptions>> = OnceLock::new();
    static EMPTY: OnceLock<Exceptions> = OnceLock::new();
    let all = S.get_or_init(|| {
        crate::data::files_under("rivens/")
            .filter(|(p, _)| *p == "rivens/exceptions.yaml")
            .map(|(p, t)| {
                serde_norway::from_str::<ExceptionsFile>(t)
                    .unwrap_or_else(|e| panic!("{p}: {e}"))
            })
            .flat_map(|f| f.families)
            .map(|r| {
                (
                    r.family,
                    Exceptions {
                        rolls: r.rolls.into_iter().map(|s| leak(s.stat)).collect(),
                        never: r.never.into_iter().map(|s| leak(s.stat)).collect(),
                    },
                )
            })
            .collect()
    });
    all.get(family).unwrap_or_else(|| EMPTY.get_or_init(Exceptions::default))
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
        // 1.1 at once, and then all three rolls are equal.
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
            // A RIVEN IS NEVER A STANCE.
            stance: None,
            // A riven fits whatever its family fits; it is never written for
            // one weapon the way an augment is.
            exclusive_to: &[],
            // A riven is generated from stats we model; nothing about it is out
            // of scope, so there is nothing to disclose.
            unmodeled: false,
            out_of_scope: false,
            id,
            // A riven's card name is the player's own — it is not a DE item, so
            // there is nothing to look up. The UI shows the riven's own label
            // and never wiki-links it (`m.riven` gates that), so the id serves.
            name: id,
            base_drain: self.drain(),
            max_rank: MAX_RANK,
            polarity: self.polarity,
            rarity: Rarity::Legendary,
            exilus: false,
            // A weapon takes ONE riven. Rivens all share
            // one family, which is the rule the pool already has for mutually
            // exclusive mods — so the picker greys the others out, the panel
            // refuses the pair, and the optimizer never enumerates a build
            // holding two, with nothing riven-specific added anywhere.
            family: Some("riven"),
            requires_weapon: None,
            excludes_weapon: Vec::new(),
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

/// WHICH RIVEN POOL THIS WEAPON ROLLS FROM — the NARROWEST of its mod pools
/// that has riven stats at all.
///
/// The rule lived in `webapi` and had one caller; a second one (canonicalising
/// a board row's elements) is what moved it here. It is the engine's answer for
/// the reason every other equip rule is: two copies of it would be two answers,
/// and this one decides whether a riven's Heat pairs with the build's Cold.
pub fn class_for_weapon(weapon: &str) -> Option<&'static str> {
    crate::weapons_data::spec(weapon)?
        .mod_pools
        .iter()
        .rev()
        .find(|c| !pool(c).is_empty())
        .map(|c| &*Box::leak(c.clone().into_boxed_str()))
}

/// A RIVEN AS A PUBLIC RECORD STATES IT: which stats it carries, and which one
/// is the malus. The ROLLS are deliberately absent.
///
/// A riven is an item that exists on one machine, which is why no board row
/// could ever hold one — and this is the shape that CAN be held, because it is
/// a statement anybody can act on: roll this weapon for these stats. What a
/// particular copy landed on is luck, and the board has never ranked luck. It
/// scores every row at full Forma, every mod at max rank and every valence at
/// the roll's ceiling for the same reason.
///
/// So a shape is scored at ITS OWN ceiling, and [`perfect`] is what finds it.
/// `Deserialize` so a BOARD row can carry one: `boards_data::BoardEntry` reads
/// the same block the scorer writes. `rolls` sits beside it in the file and is
/// not part of the shape — serde ignores it, which is right: a shape is scored
/// at its own ceiling and the rolls are what THIS engine found there.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
pub struct RivenShape {
    /// Bonus stat ids, SORTED — a riven's stats do not combine with each other,
    /// so two players listing them in different orders described one riven and
    /// must produce one row. (Mod ORDER is the opposite and stays as placed:
    /// elements pair in the order the mods sit in, which is why `canonical_mods`
    /// exists at all.)
    pub bonuses: Vec<String>,
    /// The malus, when the riven has one. A riven without one rolls smaller
    /// bonuses, so "no malus" is a different shape rather than a better one.
    pub malus: Option<String>,
}

impl RivenShape {
    /// The shape of a rolled riven — what survives when the luck is removed.
    pub fn of(spec: &RivenSpec) -> Self {
        let mut bonuses: Vec<String> = spec.bonuses.iter().map(|b| b.id.clone()).collect();
        bonuses.sort();
        Self { bonuses, malus: spec.malus.as_ref().map(|m| m.id.clone()) }
    }

    /// This shape as a rolled riven, every stat at `roll`.
    fn at(&self, class: &str, rolls: &[f64]) -> RivenSpec {
        RivenSpec {
            class: class.to_string(),
            bonuses: self
                .bonuses
                .iter()
                .zip(rolls)
                .map(|(id, &roll)| RolledStat { id: id.clone(), roll })
                .collect(),
            malus: self
                .malus
                .as_ref()
                .map(|id| RolledStat { id: id.clone(), roll: rolls[self.bonuses.len()] }),
            // AT THE CEILING, like every other investment the board scores. A
            // rank is levelled and a polarity is Forma'd, so neither is part of
            // what a row states.
            rank: MAX_RANK,
            polarity: Polarity::Madurai,
        }
    }

    /// How many stats have a roll to choose — bonuses plus the malus.
    pub fn stat_count(&self) -> usize {
        self.bonuses.len() + usize::from(self.malus.is_some())
    }

    /// THE ELEMENTS THIS SHAPE CARRIES, in the order its stats are listed.
    ///
    /// A riven with an elemental stat PAIRS with the build's other elementals,
    /// so where it sits among the mods changes the combined element and
    /// therefore the fight — the same fact that makes `builds::canonical_mods`
    /// keep mod order at all. A shape that carries none is
    /// position-independent like any other plain mod.
    ///
    /// A MALUS IS NEVER ONE. The five bonus-only stats aside, a negative
    /// elemental roll still adds that element to the pool — but no riven can
    /// take an elemental stat as its malus, which `stat_pool` already encodes
    /// (`malus: false` on those entries) and `RivenSpec::illegal` enforces.
    pub fn elements(&self, class: &str) -> Vec<crate::damage::DamageType> {
        let p = pool(class);
        self.bonuses
            .iter()
            .filter_map(|id| p.iter().find(|x| &x.id == id))
            .filter(|d| d.kind == "elemental_damage_bonus")
            .filter_map(|d| d.arg.as_deref().map(crate::weapons_data::damage_type))
            .collect()
    }
}

/// THE BEST ROLL THIS SHAPE CAN HAVE, for a caller who can score one.
///
/// Every stat goes to one END of the 0.9-1.1 band, and which end is decided by
/// the SCORE rather than by the card's sign. That distinction is the whole of
/// this function: DE's `+` and `-` describe the STAT, not the build. A riven
/// whose malus is critical chance is a BONUS on the three weapons whose
/// Incarnon form pays "+2000% damage on non-critical hits" — Felarx, Laetum
/// and Phenmor — and on those same weapons a `+` critical chance riven is
/// worst at the BOTTOM of its positive band. A per-stat table could not state
/// either case; asking the fight states both, and states the next one for free.
///
/// `score` is called at most `2^n` times, n <= 4, so at most sixteen. The
/// corners are searched exhaustively rather than by climbing: sixteen is
/// cheap, and a climb would have to assume monotonicity, which is exactly the
/// assumption this function exists to avoid making.
///
/// Ties keep the FIRST corner, which is every stat at `ROLL_MIN` — so a stat
/// the fight cannot read at all (a magazine stat on a build that never
/// reloads) comes back at the bottom of its band rather than at an arbitrary
/// end, and two runs of this cannot disagree.
pub fn perfect(shape: &RivenShape, class: &str, mut score: impl FnMut(&RivenSpec) -> f64) -> RivenSpec {
    let n = shape.stat_count();
    let n_bonus = shape.bonuses.len();
    // HOW GOOD THIS CORNER IS FOR THE PLAYER, all else equal: every bonus at its
    // ceiling and the malus at its floor. Bits 0..n_bonus are the bonuses and
    // the last one is the malus, which is the order `RivenShape::at` reads.
    let preference = |corner: u32| -> u32 {
        (0..n)
            .filter(|i| {
                let high = corner >> i & 1 == 1;
                if *i < n_bonus { high } else { !high }
            })
            .count() as u32
    };
    let mut best: Option<(f64, u32, RivenSpec)> = None;
    for corner in 0..(1u32 << n) {
        let rolls: Vec<f64> = (0..n)
            .map(|i| if corner >> i & 1 == 1 { ROLL_MAX } else { ROLL_MIN })
            .collect();
        let spec = shape.at(class, &rolls);
        let s = score(&spec);
        let p = preference(corner);
        // **A TIE GOES TO THE PLAYER**. A stat this fight
        // cannot read — Zoom, Recoil, Ammo Maximum against one standing target
        // — scores the same at both ends, and the board is publishing a riven
        // somebody will go and try to obtain. Keeping the FIRST corner means
        // every bit clear, i.e. every bonus at its MINIMUM, so a shape with one
        // dead stat would be published asking for a worse card than it needs.
        //
        // THE TIE IS EXACT, not "within noise", and that is what makes this
        // deterministic rather than a tolerance somebody picked: every corner is
        // probed under the ruler's own PINNED SEED, so two corners that differ
        // only in something the fight ignores return the same f64 bit for bit.
        // The same pairing the quick calc's gain band rests on.
        if best.as_ref().is_none_or(|(b, bp, _)| s > *b || (s == *b && p > *bp)) {
            best = Some((s, p, spec));
        }
    }
    best.map(|(_, _, spec)| spec).unwrap_or_else(|| shape.at(class, &[]))
}

#[cfg(test)]
mod tests {

/// PERFECTION IS A SEARCH OVER CORNERS, and these pin the machinery before any
/// weapon is involved: the count, the ends, and the tie rule.
#[test]
fn perfect_searches_every_corner_and_takes_the_end_the_score_likes() {
    let shape = RivenShape {
        bonuses: vec!["critical_damage".into(), "multishot".into(), "damage".into()],
        malus: Some("critical_chance".into()),
    };
    assert_eq!(shape.stat_count(), 4);

    // A SCORE THAT WANTS EVERY BONUS HIGH AND THE MALUS LOW — the ordinary
    // reading, and the one a per-stat table would have hard-coded.
    let mut seen = 0;
    let want_high = perfect(&shape, "rifle", |r| {
        seen += 1;
        r.bonuses.iter().map(|b| b.roll).sum::<f64>() - r.malus.as_ref().map_or(0.0, |m| m.roll)
    });
    assert_eq!(seen, 16, "four stats is sixteen corners");
    assert!(want_high.bonuses.iter().all(|b| b.roll == ROLL_MAX));
    assert_eq!(want_high.malus.as_ref().unwrap().roll, ROLL_MIN);

    // …AND ONE THAT WANTS THE OPPOSITE OF ALL FOUR. Nothing about the stats
    // changed — only the fight — and every end flips, which is the property
    // that makes a per-stat rule impossible.
    let want_low = perfect(&shape, "rifle", |r| {
        -(r.bonuses.iter().map(|b| b.roll).sum::<f64>())
            + r.malus.as_ref().map_or(0.0, |m| m.roll)
    });
    assert!(want_low.bonuses.iter().all(|b| b.roll == ROLL_MIN));
    assert_eq!(want_low.malus.as_ref().unwrap().roll, ROLL_MAX);

    // A STAT THE FIGHT CANNOT READ COMES BACK AT THE END THAT IS BETTER FOR THE
    // PLAYER — every bonus at its ceiling, the malus at its floor.
    //
    // Coming back at the BOTTOM is what a first-corner tiebreak gives: every
    // bit clear, so every BONUS at its minimum. Determinism is the right thing
    // to want and the wrong end to take it at — a board row is a riven somebody
    // will go and
    // try to obtain, so a shape with one dead stat was published asking for a
    // worse card than it needs.
    let flat = perfect(&shape, "rifle", |_| 1.0);
    assert!(flat.bonuses.iter().all(|b| b.roll == ROLL_MAX));
    assert_eq!(flat.malus.as_ref().unwrap().roll, ROLL_MIN);
    // …AND IT IS STILL DETERMINISTIC, which is the half worth keeping.
    let again = perfect(&shape, "rifle", |_| 1.0);
    assert_eq!(again.bonuses.iter().map(|b| b.roll).collect::<Vec<_>>(),
               flat.bonuses.iter().map(|b| b.roll).collect::<Vec<_>>());
    // …AND A REAL SCORE STILL WINS OVER THE PREFERENCE: the tie-break only
    // speaks when the fight has nothing to say, which `want_low` above proves
    // from the other side — every end flipped because the score asked for it.

    // AND IT IS SCORED AT THE CEILING OF ITS INVESTMENT, like every board row.
    assert_eq!(want_high.rank, MAX_RANK);
}

/// A SHAPE IS THE LUCK REMOVED, and two rolls of one shape are one shape.
#[test]
fn a_shape_is_what_survives_when_the_roll_is_taken_away() {
    let mk = |a: f64, b: f64| RivenSpec {
        class: "rifle".into(),
        bonuses: vec![
            RolledStat { id: "multishot".into(), roll: a },
            RolledStat { id: "damage".into(), roll: b },
        ],
        malus: Some(RolledStat { id: "recoil".into(), roll: a }),
        rank: 8,
        polarity: Polarity::Madurai,
    };
    assert_eq!(RivenShape::of(&mk(0.91, 1.07)), RivenShape::of(&mk(1.10, 0.90)));
    // SORTED, because a riven's stats do not combine with each other — two
    // players listing them in different orders described one riven.
    let shape = RivenShape::of(&mk(1.0, 1.0));
    assert_eq!(shape.bonuses, vec!["damage".to_string(), "multishot".to_string()]);
}

/// AN ELEMENTAL RIVEN PAIRS WITH THE BUILD, so the shape has to be able to say
/// which elements it brings — where it sits among the mods then changes the
/// combined element and therefore the fight.
#[test]
fn a_shape_names_the_elements_it_brings() {
    let none = RivenShape { bonuses: vec!["multishot".into(), "damage".into()], malus: None };
    assert!(none.elements("rifle").is_empty());

    let heat = RivenShape {
        bonuses: vec!["heat".into(), "multishot".into()],
        malus: Some("recoil".into()),
    };
    assert_eq!(heat.elements("rifle"), vec![crate::damage::DamageType::Heat]);

    // TWO OF THEM IS LEGAL and the pair is what a board row has to keep apart
    // from one: a riven bringing Heat AND Toxin pools two entries into the
    // element sequence, not one.
    let two = RivenShape {
        bonuses: vec!["cold".into(), "heat".into(), "multishot".into()],
        malus: None,
    };
    assert_eq!(two.elements("rifle").len(), 2);

    // A PHYSICAL stat is not an element and never pairs.
    let phys = RivenShape { bonuses: vec!["slash".into(), "damage".into()], malus: None };
    assert!(phys.elements("rifle").is_empty());
}

/// NO RIVEN TAKES AN ELEMENT AS ITS MALUS, which is what lets `elements()` read
/// the bonuses alone. Asserted against the pool rather than assumed, so a data
/// change that made one malus-legal fails here instead of silently dropping an
/// element out of the pairing.
#[test]
fn an_element_is_never_a_malus() {
    for class in crate::mods_data::classes() {
        for st in pool(class) {
            if st.kind == "elemental_damage_bonus" {
                assert!(!st.malus, "{class}/{}: an element may be a malus", st.id);
            }
        }
    }
}
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

    /// A riven's Damage is IN SERRATION'S BUCKET, not a multiplier of its own.
    ///
    /// Asked directly ("did you treat the riven's base
    /// damage as an extra multiplicative bucket?"), and the answer has to be
    /// a number rather than a reading of the code: if it were its own bucket,
    /// a riven would be worth far more than the same percentage on a mod, and
    /// every riven comparison downstream would be wrong in the same direction.
    #[test]
    fn a_rivens_damage_joins_serrations_bucket_and_does_not_multiply_it() {
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let base = WeaponBase::from_data("torid", true, &[]);
        let serration = crate::mods_data::class_pool("rifle")
            .into_iter()
            .find(|m| m.id == "serration")
            .expect("serration");
        // A riven whose Damage is deliberately NOT a round number, so an
        // accidental match cannot be luck.
        let riven = spec(&["damage", "multishot"], None, 8).to_mod_def("riven:x", 1.3);
        let bucket = |mods: &[&ModDef]| {
            resolve(&base, mods, StackPolicy::AssumedMax).base_damage_bonus
        };

        let s = bucket(&[&serration]);
        let r = bucket(&[&riven]);
        let both = bucket(&[&serration, &riven]);
        assert!(s > 1.0 && r > 1.0, "both are real bonuses: {s} {r}");
        // ADDITIVE: the bucket is a sum. A separate multiplicative bucket
        // would give (1+s)(1+r) - 1 = s + r + s*r, which for these two is
        // ~2.7 higher — not a rounding difference.
        assert!(
            (both - (s + r)).abs() < 1e-9,
            "riven + mod must SUM in one bucket: {both} vs {s} + {r} = {}",
            s + r
        );
        assert!(
            (both - (s + r + s * r)).abs() > 1.0,
            "and it is nowhere near the multiplicative reading"
        );
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

    /// A weapon does not roll a stat it does not have.
    ///
    /// Verglas Prime is the case that prompted this: a
    /// SENTINEL weapon the player never aims, hit-scan, "Ammo Max: ∞ / Ammo
    /// Type: None", 100% Cold. Its wiki table has no Zoom row and no Recoil
    /// row, so four stats plus all three physical ones are impossible on it —
    /// out of a 24-stat rifle pool, seven were being offered as choices that
    /// could never appear on a real card.
    #[test]
    fn a_weapon_does_not_roll_a_stat_it_does_not_have() {
        let v = excluded_for("verglas_prime");
        for id in ["zoom", "weapon_recoil", "ammo_maximum", "projectile_speed"] {
            assert!(v.contains(&id), "verglas_prime must not roll {id}: {v:?}");
        }
        // The wiki's 25% rule, on a weapon with no physical damage at all.
        for id in ["impact", "puncture", "slash"] {
            assert!(v.contains(&id), "verglas_prime is 100% Cold, so no {id}: {v:?}");
        }
        // …and it keeps everything a sentinel weapon really has.
        for id in ["magazine_capacity", "reload_speed", "punch_through", "cold"] {
            assert!(!v.contains(&id), "verglas_prime does have {id}: {v:?}");
        }

        // The rule is a SHARE, not "has any": Cernos Prime is 165.6/9.2/9.2,
        // so Impact stays and the two 5% components go.
        let c = excluded_for("cernos_prime");
        assert!(!c.contains(&"impact"), "impact is 90% of the arrow: {c:?}");
        assert!(c.contains(&"puncture") && c.contains(&"slash"), "both are 5%: {c:?}");
        // A projectile weapon keeps its flight speed, and one with a real ammo
        // pool keeps Ammo Maximum.
        assert!(!c.contains(&"projectile_speed") && !c.contains(&"ammo_maximum"), "{c:?}");
    }

    /// The share is read on EVERY form the weapon fires for free, not on the
    /// one the arsenal happens to show.
    ///
    /// Larkspur Prime, reported by a player through the owner:
    /// his riven is Fire Rate / Heat with a NEGATIVE Impact, and the editor
    /// would not offer Impact at all. Its beam is 10 of 90 Impact — 11%, under
    /// the 25% line — but its alt-fire, one held button away and no gauge to
    /// fill, is 140 of 420, which is 33%. One riven covers both, so the pool
    /// is the union.
    #[test]
    fn a_free_alt_fire_counts_toward_the_physical_share() {
        // BOTH ENTRIES, because the report came back on the OTHER one. This
        // asserted `larkspur_prime` alone — the card the owner relayed — and a
        // player reported the plain Larkspur refusing negative Impact a
        // fortnight later. It was already right, and a
        // family where one member is pinned and its twin is not is precisely
        // how a fixed bug gets re-reported: there is nothing to point at.
        for id in ["larkspur", "larkspur_prime"] {
            let l = excluded_for(id);
            assert!(!l.contains(&"impact"), "{id}: the alt-fire is 33% Impact: {l:?}");
            // Nothing else is invented: neither form deals Puncture or Slash.
            assert!(l.contains(&"puncture") && l.contains(&"slash"), "{id}: {l:?}");
        }

        // The OTHER rule is the flight one, and here the derivation is only
        // the fallback. Phantasma Prime's plasma bomb genuinely flies at
        // 25 m/s, which by the derivation alone would keep Projectile Speed
        // — and 500 real Phantasma rivens carry it zero times, so the survey
        // overrules the reasoning. Gotva Prime is the same claim with
        // no survey behind it: its family is the one warframe.market refused,
        // so the derivation still answers for it.
        let p = excluded_for("phantasma_prime");
        assert!(p.contains(&"projectile_speed"), "0 of 500 Phantasma cards: {p:?}");
        for id in ["burston_prime", "gotva_prime", "karak_wraith", "prisma_grinlok"] {
            let e = excluded_for(id);
            assert!(e.contains(&"projectile_speed"), "{id} is hit-scan in every form: {e:?}");
        }
    }



    /// A FAMILY IS CALLED WHAT DE CALLS IT.
    ///
    /// `riven_family` decides which weapons share a pool, which an
    /// `exceptions.yaml` entry covers, and whether the family can be surveyed
    /// at all, since the market is queried by that name. So a name nobody else
    /// uses silently makes the family a singleton and the survey empty.
    ///
    /// "Strip the variant prefix" is right for a Prime, a Vandal, a Wraith, a
    /// Prisma, a Rakta or a Telos — DE's list holds `Boltor` and no `Boltor
    /// Prime` — and WRONG for a weapon with no ordinary counterpart, where the
    /// prefix IS the name: `Kuva Ayanga`, `Gotva Prime`, `Vadarya Prime`, `Coda
    /// Bassocyst`, `Dual Coda Torxica`, `EFV-5 Jupiter`. It caught six, one of
    /// which is why `pools.yaml` records "Gotva: NOT SURVEYED (the API
    /// refused)" — it was asked about a weapon that does not exist.
    ///
    /// THE SNAPSHOT CONFIRMS AND NEVER REFUTES. `de_families.yaml` is one week
    /// of trades, so a family absent from it is a family nobody traded, not a
    /// family that does not exist. This asserts the names DE DOES know and
    /// reports the rest.
    #[test]
    fn every_riven_family_is_spelled_the_way_de_spells_it() {
        let de: std::collections::HashSet<&str> =
            de_families().iter().map(String::as_str).collect();
        assert!(de.len() > 300, "the snapshot loaded: {}", de.len());
        let mut families: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
        for w in crate::weapons_data::all() {
            if let Some(f) = w.riven_family.as_deref() {
                families.entry(f).or_default().push(w.id.as_str());
            }
        }
        // A CASE-FOLDED MATCH IS STILL A MISMATCH, and it is the interesting
        // one: the schema says "in all capital case" and the dump is title
        // case, so a spelling that differs only in capitalisation is a sign the
        // name came from somewhere other than DE.
        let folded: std::collections::HashMap<String, &str> =
            de.iter().map(|x| (x.to_lowercase(), *x)).collect();
        let mut wrong = Vec::new();
        let mut untraded = 0;
        for (f, members) in &families {
            if de.contains(f) {
                continue;
            }
            match folded.get(&f.to_lowercase()) {
                Some(right) => wrong.push(format!("{f:?} should be {right:?} ({members:?})")),
                None => untraded += 1,
            }
        }
        assert!(
            wrong.is_empty(),
            "{} family names disagree with DE's own `compatibility`:\n  {}",
            wrong.len(),
            wrong.join("\n  ")
        );
        // …AND THE COVERAGE IS ASSERTED, not merely printed. Every family in
        // this roster was traded in the surveyed week, so an untraded one is
        // new information — a weapon so obscure nobody trades its rivens, or a
        // name that is wrong in a way case-folding cannot see. Either is worth
        // being told about rather than discovering later.
        assert_eq!(
            untraded, 0,
            "{untraded} of {} families are not in DE's snapshot at all",
            families.len()
        );
    }

    /// A RIVEN FAMILY AGREES WITH ITSELF, because a riven belongs to the family
    /// and not to a member of it.
    ///
    /// One card equips on every member — a Ballistica riven IS a Ballistica
    /// Prime riven and a Rakta Ballistica riven, which is what `riven_family`
    /// means and why the exception table and the survey are both keyed on it.
    /// So a pool derived from one member describes a card that cannot exist,
    /// and the app tells a player holding the real thing that it is "not a
    /// legal riven".
    ///
    /// FIFTEEN FAMILIES DISAGREED WITH THEMSELVES. The sharpest is the
    /// Ballistica: the Prime's charged shot is over the 25% Slash line and the
    /// other two members are under it, so the same card was legal on one entry
    /// and refused on the other two.
    ///
    /// ASSERTED OVER EVERY FAMILY rather than over the fifteen. A list of names
    /// cannot report the sixteenth, and the sixteenth arrives the day a weapon
    /// joins a family with a damage split of its own — which is a thing DE does
    /// (the Tenet and Kuva variants in this list are all exactly that).
    #[test]
    fn a_riven_family_agrees_with_itself() {
        use std::collections::BTreeMap;
        let mut by_family: BTreeMap<&str, Vec<(&str, Vec<&'static str>)>> = BTreeMap::new();
        for w in crate::weapons_data::all() {
            let Some(f) = w.riven_family.as_deref() else { continue };
            let mut e = excluded_for(&w.id);
            e.sort_unstable();
            by_family.entry(f).or_default().push((w.id.as_str(), e));
        }
        assert!(by_family.len() > 200, "every family is asked: {}", by_family.len());
        let mut split = Vec::new();
        for (f, members) in &by_family {
            if members.iter().any(|(_, e)| *e != members[0].1) {
                split.push(format!(
                    "{f}: {}",
                    members
                        .iter()
                        .map(|(id, e)| format!("{id}{e:?}"))
                        .collect::<Vec<_>>()
                        .join(" vs ")
                ));
            }
        }
        assert!(
            split.is_empty(),
            "a riven equips on every member of its family, so the pool cannot \
             differ between members — {} families disagree:\n  {}",
            split.len(),
            split.join("\n  ")
        );

        // …AND THE FAMILY IS THE UNION, not the intersection. A test that only
        // asserted agreement would pass just as well on a derivation that
        // refused everything to everybody, so this names what the union WON:
        // the Ballistica Prime's charged shot is 18% Slash on a 44% Puncture
        // body, over the line, and all three members roll it now.
        for id in ["ballistica", "ballistica_prime", "rakta_ballistica"] {
            let e = excluded_for(id);
            assert!(!e.contains(&"slash"), "{id}: the Prime's charge earns Slash: {e:?}");
            // Nothing is invented: no member is over the line on Impact.
            assert!(e.contains(&"impact"), "{id}: no member deals 25% Impact: {e:?}");
        }
        // The Ogris is the other direction of the same rule — the KUVA member
        // is the one over the line, and the ordinary one inherits it.
        assert!(!excluded_for("ogris").contains(&"impact"));
        assert!(!excluded_for("ogris").contains(&"puncture"));
    }

    /// THE FAMILY POOL IS THE UNION, and these are the cards that say so.
    ///
    /// A riven equips on every member of its family, so a family whose members
    /// disagree under the 25% line has two readings the rule cannot choose
    /// between: UNION (one member over the line earns it for all) or
    /// INTERSECTION. Only cards can answer it, and among the SURVEYED families
    /// seven (family, stat) pairs disagree — the listings say UNION on five
    /// (Boar Slash, Braton Impact, Braton Puncture, Karak Slash, Sybaris
    /// Puncture) and INTERSECTION on two, which are ONE family already carrying
    /// its own exception. So the union is the rule and the Sicarus is the
    /// exception — which is the shape this whole file is built on, arrived at
    /// from the other end.
    ///
    /// EACH HALF IS ASSERTED ON ITS OWN CALL, and the first version was not.
    /// Four of the five UNION families are also in `exceptions.yaml` — the same
    /// survey answered the question and wrote those entries — so `excluded_for`
    /// returns the exception's answer whatever the rule does, and the test
    /// passed on a derivation sabotaged to take the INTERSECTION. The union
    /// half asks `derived_for`, which is the rule alone; the Sicarus half asks
    /// `excluded_for`, which is the rule plus the exception that overrules it.
    #[test]
    fn a_family_pool_is_the_union_and_the_sicarus_is_the_exception() {
        // The five the cards earn through ONE member being over the line —
        // against the RULE, so an intersection here cannot hide behind an
        // exception written from the same survey.
        for (id, stat) in [
            ("boar", "slash"),
            ("boar_prime", "slash"),
            ("braton", "impact"),
            ("braton_prime", "impact"),
            ("mk1_braton", "impact"),
            ("braton_vandal", "puncture"),
            ("karak", "slash"),
            ("karak_wraith", "slash"),
            ("kuva_karak", "slash"),
            ("dex_sybaris", "puncture"),
        ] {
            let e = derived_for(id);
            assert!(
                !e.contains(&stat),
                "{id} rolls {stat} — a family member is over the line and real \
                 cards carry it, so the RULE must be the union: {e:?}"
            );
        }
        // …and the two the cards refuse, on BOTH members, through the
        // exception rather than through the derivation.
        for id in ["sicarus", "sicarus_prime"] {
            let e = excluded_for(id);
            assert!(
                e.contains(&"puncture") && e.contains(&"slash"),
                "{id}: 0 of 500 live listings carry either, which is what the \
                 exception records — {e:?}"
            );
        }
    }

    /// AN INCARNON FORM DOES NOT WIDEN THE POOL, and this is what counting it
    /// would cost — in real cards, per family, rather than in argument.
    ///
    /// The rule above reads the union of the forms a weapon fires FOR FREE, and
    /// the question the union raises is where "for free" stops. A gauge-switched
    /// form is paid for with evolutions and a riven's pool is fixed when it
    /// drops, which is the reasoning; the reasoning is not what settles it.
    ///
    /// Removing the `is_adapter_form` filter moves 25 weapons, and seven of
    /// them would gain a PHYSICAL stat that the survey of live listings records
    /// **zero** times:
    ///
    /// | family | stat the Incarnon form would unlock | cards carrying it |
    /// | --- | --- | --- |
    /// | Boltor | Slash | 0 of 500 |
    /// | Latron | Impact | 0 of 500 |
    /// | Atomos | Impact | 0 of 500 |
    /// | Lex | Impact | 0 of 500 |
    /// | Dual Toxocyst | Slash | 0 of 500 |
    /// | Kunai | Slash | 0 of 430 |
    /// | Bronco | Slash | 0 of 309 |
    ///
    /// About 3,200 listings, not one of them carrying a stat the wider rule
    /// would offer. That is the same shape of evidence the flight rule already
    /// rests on (the Latron, Lex and Atomos Incarnon forms all fire a literal
    /// travelling projectile and their families show 0, 4 and 0 Projectile
    /// Speed cards) — so it is not two arguments, it is one finding on both
    /// halves of the derivation.
    ///
    /// THE SURVEY IS EVIDENCE AND NOT A LAW, and `data/rivens/pools.yaml` says
    /// so itself: *"absence in 500 listings is strong evidence and not a
    /// guarantee. An in-game card that contradicts a `never` here beats the
    /// file."* One real card carrying negative Slash on a Boltor settles this
    /// the other way, and the way to record it is an entry in
    /// `exceptions.yaml` — the same route the Furis took.
    ///
    /// This test exists so that flipping the rule is a decision with a price
    /// tag on it rather than a one-line edit that quietly reddens nothing.
    #[test]
    fn an_incarnon_form_does_not_widen_the_physical_pool() {
        // Each pair is (weapon, the stat its Incarnon form would unlock). The
        // assertion is that the stat is still EXCLUDED — that is, that the
        // gauge-switched form was not counted.
        for (id, stat, cards) in [
            ("boltor", "slash", 500),
            ("boltor_prime", "slash", 500),
            ("telos_boltor", "slash", 500),
            ("latron", "impact", 500),
            ("latron_prime", "impact", 500),
            ("latron_wraith", "impact", 500),
            ("atomos", "impact", 500),
            ("lex", "impact", 500),
            ("lex_prime", "impact", 500),
            ("dual_toxocyst", "slash", 500),
            ("kunai", "slash", 430),
            ("mk1_kunai", "slash", 430),
            ("bronco", "slash", 309),
        ] {
            let e = excluded_for(id);
            assert!(
                e.contains(&stat),
                "{id}: counting the Incarnon form would offer {stat}, which \
                 0 of {cards} real cards in this family carry — {e:?}"
            );
        }
        // …AND THE NEGATIVE CONTROL, which is what keeps this from being a test
        // that would pass on a derivation that excluded everything: the same
        // weapons still roll the physical stats their BASE form earns.
        let lex = excluded_for("lex");
        assert!(!lex.contains(&"puncture"), "the Lex is 88% Puncture: {lex:?}");
        let kunai = excluded_for("kunai");
        assert!(!kunai.contains(&"puncture"), "the Kunai is 90% Puncture: {kunai:?}");
        // …and the FREE alt-fire is still counted, which is the rule this one
        // bounds rather than replaces.
        assert!(!excluded_for("larkspur").contains(&"impact"));
    }

    /// AN EXCEPTION OVERRIDES THE RULES, and only an exception does.
    ///
    /// The survey is not in this path. Every answer it gave is an entry in
    /// `exceptions.yaml` carrying the count it came from, so this asserts the
    /// ANSWERS, which is what a player sees, rather than
    /// which file produced them.
    #[test]
    fn an_exception_overrides_the_derivation_and_nothing_else_does() {
        // 1. A REAL CARD. The Furis is hit-scan in both forms, so the rules
        //    refuse Projectile Speed; a player has the card.
        let f = excluded_for("furis");
        assert!(!f.contains(&"projectile_speed"), "a real Furis card carries it: {f:?}");
        // The MK1 is the same riven, because it is the same family.
        assert!(!excluded_for("mk1_furis").contains(&"projectile_speed"));

        // 2. ADDING what the rules refused. The Ocucor is 9% Puncture and 91%
        //    Radiation — the 25% rule strikes all three physical stats, and all
        //    three roll on real cards.
        let o = excluded_for("ocucor");
        for id in ["impact", "puncture", "slash", "projectile_speed"] {
            assert!(!o.contains(&id), "the Ocucor rolls {id}: {o:?}");
        }
        // …and TAKING AWAY what they allowed. The Phenmor is 30% Puncture, over
        // the line, and no live listing carries it.
        assert!(excluded_for("phenmor").contains(&"puncture"));
        // Zoom is not derived at all — nothing in the weapon data says a Boar
        // has no scope, so only an exception can say it.
        assert!(excluded_for("boar").contains(&"zoom"));
        // A share landing EXACTLY on 25% is decided by neither of us: Karak
        // Wraith's Slash is 7.75 of 31 and the rule reads "more than 25%".
        assert!(!excluded_for("karak_wraith").contains(&"slash"));

        // 3. THE DERIVATION answers for everything unexcepted — 15 of the 26
        //    families have no entry at all, and a weapon added tomorrow is
        //    approximately right before anyone looks at a card.
        let v = excluded_for("verglas_prime");
        assert!(v.contains(&"zoom") && v.contains(&"impact"));
        assert!(exceptions("Verglas").rolls.is_empty() && exceptions("Verglas").never.is_empty());
    }

    /// THE SURVEY IS A CHECK, NOT A SOURCE — this is the check.
    ///
    /// It exists because the opposite arrangement fails silently. With
    /// `pools.yaml` outranking the derivation, a re-run of the scrape that came
    /// back "nothing rolls anything" for all 26 families would empty every
    /// pool in the app, and the only thing that noticed was two tests about
    /// something else.
    ///
    /// Now a disagreement is a FAILURE that names the family and the stat, and
    /// the fix is a human one: promote it into `exceptions.yaml` with its count,
    /// or fix the rule. A broken scrape fails this immediately and loudly,
    /// because a broken scrape disagrees with everything at once.
    #[test]
    fn the_survey_still_agrees_with_the_rules() {
        let mut checked = 0;
        let mut bad: Vec<String> = Vec::new();
        for w in crate::weapons_data::all() {
            let Some(fam) = w.riven_family.as_deref() else { continue };
            let Some(sv) = survey(fam) else { continue };
            let ours = excluded_for(&w.id);
            checked += 1;
            for r in &sv.rollable {
                if ours.contains(r) {
                    bad.push(format!("{fam}/{r}: {} listings carry it, we refuse it", sv.n));
                }
            }
            for n in &sv.never {
                if !ours.contains(n) {
                    bad.push(format!("{fam}/{n}: no listing carries it, we offer it"));
                }
            }
        }
        assert!(checked > 0, "no surveyed family was reached");
        assert!(
            bad.is_empty(),
            "the survey and the rules disagree — promote each into \
             data/rivens/exceptions.yaml with its count, or fix the rule:\n  {}",
            bad.join("\n  ")
        );
    }

    /// A SURVEY THAT SAYS NOTHING ROLLS ANYTHING IS A BROKEN SCRAPE.
    ///
    /// The literal failure of 2026-08-08: the per-stat queries started coming
    /// back empty, so every family was written with an empty `rollable` and a
    /// `never` listing the entire stat table. That is not a discovery about
    /// Warframe, it is a discovery about the endpoint — and it has to be named
    /// as one before anybody reads the numbers.
    #[test]
    fn a_survey_that_refuses_everything_is_a_broken_scrape() {
        for s in surveys() {
            assert!(
                !s.rollable.is_empty(),
                "{}: the survey says nothing rolls — that is the scrape failing, \
                 not the game (n={})",
                s.family,
                s.n
            );
        }
    }

    /// Faction damage prints as a MULTIPLIER, because that is what the card
    /// says: a malus the sim stores as -0.41 reads "x0.59 Damage to Corpus", so its range runs 0.xx-1.xx and never shows a
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
    /// read a second one off a riven they own. What is NOT
    /// rounded is the arithmetic — the roll behind a displayed 144.8 is
    /// whatever 144.8 implies, exactly.
    #[test]
    fn the_reading_is_the_cards_precision_and_the_maths_is_not() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let multishot = by("multishot");
        assert_eq!(multishot.decimals(), 1);
        assert_eq!(multishot.print(1.447_87), "+144.8% Multishot");
        assert_eq!(by("damage_to_corpus").decimals(), 2, "the card's own two");

        // The value behind it is untouched: `shown` is exact and the roll a
        // typed reading implies is exact too, one decimal in or not. Read off
        // the middle of the band so the case is an ordinary roll and not a
        // clamp — the band moves with shape and disposition, so it is asked
        // for rather than written down.
        let s = spec(&["multishot", "damage"], None, 8);
        let (lo, hi) = s.bounds_of(multishot, true, 1.3);
        let reading = (multishot.shown((lo + hi) / 2.0) * 10.0).round() / 10.0;
        let r = s.roll_for_value(multishot, true, 1.3, multishot.from_shown(reading));
        assert!((multishot.shown(s.value_of(multishot, r, true, 1.3)) - reading).abs() < 1e-9);
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
    /// every stat can sit at 1.1 at once. The name must then still be one name, and must not depend
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
    /// astronomically unlikely. It has to be constructible, because it is the CEILING, which
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

/// A RIVEN IS THE WEAPON FAMILY'S, so every member of a family has to offer the
/// same card — **within one riven CLASS**.
///
/// The wiki states the mechanic and the app files rivens by family (page,
/// 2026-08-25): *"Riven mods can be used on variants of a particular weapon,
/// including MK1, Prime, Vandal, Wraith, Dex, Prisma, Mara, and Syndicate
/// variants"*. That makes the pool a property of the FAMILY rather than of the
/// entry, and this asserts the data agrees — a stat one variant can roll and
/// another cannot would be a card the editor offers on both, the board accepts
/// on one, and refuses on the other.
///
/// THE CLASS IS PART OF THE KEY, and a KITGUN is why. `tombfinger_primary` and
/// `tombfinger_secondary` are one family and two riven types: built as a
/// primary the chamber takes a RIFLE riven, built as a secondary a PISTOL one,
/// which is a different card with a different pool. Grouping by family alone
/// makes those two look like a disagreement inside one family when they are
/// two families' worth of card that happen to share a name — and it is what
/// this test caught the day the page started filing by family.
///
/// Derived from the roster rather than from a list of families, so a weapon
/// added tomorrow is covered by nobody.
#[cfg(test)]
mod riven_family_tests {
    use std::collections::BTreeMap;

    /// One weapon's view of a card: its id, and the stats it may NOT roll.
    type Member = (&'static str, Vec<&'static str>);

    #[test]
    fn every_member_of_a_riven_family_rolls_the_same_pool() {
        // Keyed by the CARD — a (family, riven class) pair, not a family.
        let mut by_card: BTreeMap<(&str, &str), Vec<Member>> = BTreeMap::new();
        for w in crate::weapons_data::all() {
            let Some(family) = w.riven_family.as_deref() else { continue };
            let class = super::class_for_weapon(&w.id).unwrap_or("");
            let mut excluded = super::excluded_for(&w.id);
            excluded.sort_unstable();
            by_card
                .entry((family, class))
                .or_default()
                .push((w.id.as_str(), excluded));
        }
        let mut bad = Vec::new();
        for ((family, class), members) in &by_card {
            let (first_id, first_excluded) = &members[0];
            for (id, excluded) in &members[1..] {
                if excluded != first_excluded {
                    bad.push(format!(
                        "{family} ({class}): {first_id} excludes {first_excluded:?}                          but {id} excludes {excluded:?}"
                    ));
                }
            }
        }
        assert!(bad.is_empty(), "riven pools differ inside one card:
{}", bad.join("
"));
        // A FLOOR, so the sweep cannot pass by finding nothing: the roster has
        // plenty of families with several members and this must be looking at
        // them rather than at a roster it failed to read.
        let shared = by_card.values().filter(|m| m.len() > 1).count();
        assert!(shared > 20, "only {shared} cards are shared by more than one weapon");
    }
}
