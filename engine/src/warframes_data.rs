//! WARFRAMES — the builder's frame, and everything a build seats on it.
//!
//! A Warframe build is eight mods, an exilus, an aura, two arcanes, five archon
//! shards and at most one Helminth infusion. [`resolve`] turns one into the
//! numbers the arsenal shows and the four abilities at those numbers. Nothing
//! here reaches a fight; the rules and their sources are `docs/WARFRAMES.md`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::auras_data::AuraEffect;
use crate::shards_data::ShardPick;

/// A number the arsenal shows for a Warframe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FrameStat {
    Health,
    Shield,
    Armor,
    Energy,
    SprintSpeed,
    AbilityStrength,
    AbilityDuration,
    AbilityEfficiency,
    AbilityRange,
    CastingSpeed,
}

impl FrameStat {
    pub const ALL: [FrameStat; 10] = [
        FrameStat::Health,
        FrameStat::Shield,
        FrameStat::Armor,
        FrameStat::Energy,
        FrameStat::SprintSpeed,
        FrameStat::AbilityStrength,
        FrameStat::AbilityDuration,
        FrameStat::AbilityEfficiency,
        FrameStat::AbilityRange,
        FrameStat::CastingSpeed,
    ];

    pub fn id(self) -> &'static str {
        match self {
            FrameStat::Health => "health",
            FrameStat::Shield => "shield",
            FrameStat::Armor => "armor",
            FrameStat::Energy => "energy",
            FrameStat::SprintSpeed => "sprint_speed",
            FrameStat::AbilityStrength => "ability_strength",
            FrameStat::AbilityDuration => "ability_duration",
            FrameStat::AbilityEfficiency => "ability_efficiency",
            FrameStat::AbilityRange => "ability_range",
            FrameStat::CastingSpeed => "casting_speed",
        }
    }

    /// The arsenal's own label.
    pub fn label(self) -> &'static str {
        match self {
            FrameStat::Health => "Health",
            FrameStat::Shield => "Shield Capacity",
            FrameStat::Armor => "Armor",
            FrameStat::Energy => "Energy Max",
            FrameStat::SprintSpeed => "Sprint Speed",
            FrameStat::AbilityStrength => "Ability Strength",
            FrameStat::AbilityDuration => "Ability Duration",
            FrameStat::AbilityEfficiency => "Ability Efficiency",
            FrameStat::AbilityRange => "Ability Range",
            FrameStat::CastingSpeed => "Casting Speed",
        }
    }

    /// The four ability stats are shown as percentages of a 100% base; the rest
    /// are the frame's own quantities.
    pub fn is_ratio(self) -> bool {
        matches!(
            self,
            FrameStat::AbilityStrength
                | FrameStat::AbilityDuration
                | FrameStat::AbilityEfficiency
                | FrameStat::AbilityRange
                | FrameStat::CastingSpeed
        )
    }

    /// The yaml `kind:` that adds to it is `<id>_bonus`.
    fn from_kind(kind: &str) -> Option<FrameStat> {
        let id = kind.strip_suffix("_bonus")?;
        FrameStat::ALL.into_iter().find(|s| s.id() == id)
    }

    fn parse(id: &str) -> FrameStat {
        FrameStat::ALL
            .into_iter()
            .find(|s| s.id() == id)
            .unwrap_or_else(|| panic!("unknown Warframe stat: {id}"))
    }
}

/// WHAT A BUILD CAN DO FOR SURVIVAL, as a tag rather than a number. A frame's
/// build answers too many questions to rank by one score, so a build states which
/// of these it carries and where each comes from. docs/WARFRAMES.md §Tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    /// Takes no damage at all for a while.
    Invulnerable,
    /// Removes status effects already on the Warframe.
    StatusCleanse,
    /// No NEW status effect lands; what is already there stays.
    StatusImmunity,
    /// Damage taken has a hard ceiling.
    DamageCap,
}

impl Capability {
    pub const ALL: [Capability; 4] = [
        Capability::Invulnerable,
        Capability::StatusCleanse,
        Capability::StatusImmunity,
        Capability::DamageCap,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Capability::Invulnerable => "invulnerable",
            Capability::StatusCleanse => "status_cleanse",
            Capability::StatusImmunity => "status_immunity",
            Capability::DamageCap => "damage_cap",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Capability::Invulnerable => "Invulnerable",
            Capability::StatusCleanse => "Status cleanse",
            Capability::StatusImmunity => "Status immunity",
            Capability::DamageCap => "Damage cap",
        }
    }

    fn parse(path: &str, id: &str) -> Capability {
        Capability::ALL
            .into_iter()
            .find(|c| c.id() == id)
            .unwrap_or_else(|| panic!("{path}: unknown tag `{id}`"))
    }
}

/// One item's claim to a tag, and when it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct TagGrant {
    pub tag: Capability,
    /// The condition and timing, in the card's own terms ("on roll, 3 s").
    pub when: String,
    /// An ABILITY's tag as the Helminth infuses it: `None` = unchanged, and
    /// `Some(None)` = the infused version does not grant it at all.
    pub infused: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct RawTag {
    tag: String,
    when: String,
    /// The infused version's timing, where its page states a different one.
    #[serde(default)]
    infused_when: Option<String>,
    /// The infused version grants no such tag (Omamori).
    #[serde(default)]
    not_when_infused: bool,
}

fn tags_of(path: &str, raw: &[RawTag]) -> Vec<TagGrant> {
    raw.iter()
        .map(|t| TagGrant {
            tag: Capability::parse(path, &t.tag),
            when: t.when.clone(),
            infused: if t.not_when_infused {
                Some(None)
            } else {
                t.infused_when.clone().map(Some)
            },
        })
        .collect()
}

// ---- focus ----------------------------------------------------------------

/// One Focus node that reaches the Warframe, at max rank.
#[derive(Debug, Clone)]
pub struct FocusNode {
    pub id: String,
    pub name: String,
    pub text: String,
    /// No Operator action is needed; the node always applies.
    pub always: bool,
    pub when: String,
    pub effects: Vec<FrameEffect>,
    pub tags: Vec<TagGrant>,
}

/// A Focus school. Only the ACTIVE school's nodes apply: "Active and Passive ways
/// are only usable in the specific focus school they belong to" (W`Focus`).
#[derive(Debug, Clone)]
pub struct FocusSchool {
    pub id: String,
    pub name: String,
    pub nodes: Vec<FocusNode>,
    /// The school's Tektolyst Artifact: one per school, seated by the Operator.
    pub artifact: Option<ArtifactDef>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactDef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct RawNode {
    id: String,
    name: String,
    text: String,
    #[serde(default)]
    always: bool,
    #[serde(default)]
    when: String,
    #[serde(default)]
    effects: Vec<RawEffect>,
    #[serde(default)]
    tags: Vec<RawTag>,
}

#[derive(Debug, Deserialize)]
struct RawSchool {
    id: String,
    name: String,
    nodes: Vec<RawNode>,
    #[serde(default)]
    artifact: Option<ArtifactDef>,
    #[serde(default)]
    source: SourceFile,
}

pub fn focus_schools() -> &'static [FocusSchool] {
    static F: OnceLock<Vec<FocusSchool>> = OnceLock::new();
    F.get_or_init(|| {
        crate::data::files_under("focus/")
            .map(|(p, text)| {
                let r: RawSchool = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                FocusSchool {
                    id: r.id,
                    name: r.name,
                    nodes: r
                        .nodes
                        .iter()
                        .map(|n| {
                            assert!(n.always || !n.when.is_empty(), "{p}: {} says neither `always` nor `when`", n.id);
                            FocusNode {
                                id: n.id.clone(),
                                name: n.name.clone(),
                                text: n.text.clone(),
                                always: n.always,
                                when: n.when.clone(),
                                effects: n.effects.iter().map(|e| effect(p, e)).collect(),
                                tags: tags_of(p, &n.tags),
                            }
                        })
                        .collect(),
                    artifact: r.artifact,
                    url: r.source.url,
                }
            })
            .collect()
    })
}

pub fn focus_school(id: &str) -> Option<&'static FocusSchool> {
    focus_schools().iter().find(|s| s.id == id)
}

// ---- the Tektolyst Artifact ------------------------------------------------

/// "Each Tektolyst Artifact has 5 mod slots and 1 arcane slot." (W`Tektolyst_Artifact`)
pub const ARTIFACT_MOD_SLOTS: usize = 5;

/// An Antique mod. Every one is Universal with a base drain of 0 (the wiki's mod
/// module), so an artifact has no capacity to spend and no polarity to match.
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactMod {
    pub id: String,
    pub name: String,
    /// The school the card belongs to; any artifact seats it.
    pub school: String,
    pub rarity: String,
    pub max_rank: u32,
    #[serde(default)]
    pub internal_name: Option<String>,
    /// The card at max rank; a card with a `bonus` states it on the last line.
    pub description: String,
    #[serde(default)]
    pub bonus: Option<ArtifactBonus>,
    #[serde(default, rename = "source", deserialize_with = "source_url")]
    pub url: Option<String>,
}

/// A card's last line: `value` `unit` `stat` for each thing `per` counts.
#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactBonus {
    /// `unique_school` ("for each Mod from a unique School") or a school id
    /// ("for each Unairu School Mod").
    pub per: String,
    pub value: f64,
    #[serde(default)]
    pub unit: String,
    pub stat: String,
}

impl ArtifactMod {
    /// How many times the bonus pays with `seated` on the artifact. A school id
    /// counts that school's cards; `unique_school` counts each OTHER school
    /// seated once, never the card's own (MEASUREMENTS M93).
    pub fn bonus_count(&self, seated: &[&ArtifactMod]) -> u32 {
        let Some(b) = &self.bonus else { return 0 };
        if b.per == "unique_school" {
            let mut schools: Vec<&str> =
                seated.iter().map(|m| m.school.as_str()).filter(|s| *s != self.school).collect();
            schools.sort_unstable();
            schools.dedup();
            schools.len() as u32
        } else {
            seated.iter().filter(|m| m.school == b.per).count() as u32
        }
    }

    /// The card with `seated` on the artifact: its last line is the bonus paid out.
    pub fn card_with(&self, seated: &[&ArtifactMod]) -> Vec<String> {
        let mut lines: Vec<String> = self.description.lines().map(str::to_string).collect();
        if let (Some(b), Some(last)) = (&self.bonus, lines.last_mut()) {
            let total = b.value * f64::from(self.bonus_count(seated));
            *last = format!("+{total}{} {}", b.unit, b.stat);
        }
        lines
    }
}

/// Why an artifact cannot hold `a`, one reason per problem.
pub fn artifact_refusals(a: &ArtifactPick) -> Vec<String> {
    let mut out = Vec::new();
    if a.mods.len() > ARTIFACT_MOD_SLOTS {
        out.push(format!("an artifact has {ARTIFACT_MOD_SLOTS} mod slots, not {}", a.mods.len()));
    }
    for (i, id) in a.mods.iter().enumerate() {
        if artifact_mod_by_id(id).is_none() {
            out.push(format!("unknown artifact mod: {id}"));
        } else if a.mods[..i].contains(id) {
            out.push(format!("artifact mod seated twice: {id}"));
        }
    }
    if let Some(id) = a.arcane.as_deref().filter(|id| artifact_arcane_by_id(id).is_none()) {
        out.push(format!("unknown artifact arcane: {id}"));
    }
    out
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactArcane {
    pub id: String,
    pub name: String,
    pub rarity: String,
    pub max_rank: u32,
    #[serde(default)]
    pub internal_name: Option<String>,
    pub description: String,
    #[serde(default, rename = "source", deserialize_with = "source_url")]
    pub url: Option<String>,
}

fn source_url<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(SourceFile::deserialize(d)?.url)
}

fn load_sorted<T: serde::de::DeserializeOwned>(prefix: &str, id: fn(&T) -> &str) -> Vec<T> {
    let mut out: Vec<T> = leak_all(prefix)
        .map(|(p, text)| serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}")))
        .collect();
    out.sort_by(|a, b| id(a).cmp(id(b)));
    out
}

pub fn artifact_mods() -> &'static [ArtifactMod] {
    static M: OnceLock<Vec<ArtifactMod>> = OnceLock::new();
    M.get_or_init(|| load_sorted("artifact_mods/", |m: &ArtifactMod| &m.id))
}

pub fn artifact_arcanes() -> &'static [ArtifactArcane] {
    static A: OnceLock<Vec<ArtifactArcane>> = OnceLock::new();
    A.get_or_init(|| load_sorted("artifact_arcanes/", |a: &ArtifactArcane| &a.id))
}

pub fn artifact_mod_by_id(id: &str) -> Option<&'static ArtifactMod> {
    artifact_mods().iter().find(|m| m.id == id)
}

pub fn artifact_arcane_by_id(id: &str) -> Option<&'static ArtifactArcane> {
    artifact_arcanes().iter().find(|a| a.id == id)
}

/// One line of a mod's or an arcane's card, typed.
#[derive(Debug, Clone, PartialEq)]
pub enum FrameEffect {
    /// A percentage bonus at MAX rank, additive with every other source of it.
    Bonus(FrameStat, f64),
    /// A flat amount added after the multiplier (`<stat>_flat`, `value:`).
    Flat(FrameStat, f64),
    /// "x0.20 Max Shield Capacity": a multiplier on the finished shields, per rank.
    ShieldMultiplier(Vec<f64>),
    /// A fixed shield-gate length, per rank (Catalyzing Shields).
    ShieldGateSeconds(Vec<f64>),
    /// A line of the card this panel does not compute, verbatim.
    Unmodelled(String),
    /// A line that cannot pay out in a Warframe's own panel, and why.
    OutOfScope(String),
}

#[derive(Debug, Default, Deserialize)]
struct SourceFile {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawEffect {
    kind: String,
    #[serde(rename = "rankMax", default)]
    rank_max: f64,
    #[serde(default)]
    value: f64,
    /// A per-rank ladder, where a card's values do not follow `at_rank`.
    #[serde(default)]
    ranks: Vec<f64>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    applies_to: Option<String>,
}

fn effect(path: &str, e: &RawEffect) -> FrameEffect {
    match e.kind.as_str() {
        "unmodelled" => FrameEffect::Unmodelled(
            e.text.clone().unwrap_or_else(|| panic!("{path}: an unmodelled line carries its text")),
        ),
        "out_of_scope" => FrameEffect::OutOfScope(e.applies_to.clone().unwrap_or_default()),
        "max_shield_multiplier" | "shield_gate_seconds" => {
            assert!(!e.ranks.is_empty(), "{path}: `{}` carries its `ranks`", e.kind);
            if e.kind == "max_shield_multiplier" {
                FrameEffect::ShieldMultiplier(e.ranks.clone())
            } else {
                FrameEffect::ShieldGateSeconds(e.ranks.clone())
            }
        }
        kind if kind.ends_with("_flat") => {
            let id = kind.trim_end_matches("_flat");
            match FrameStat::ALL.into_iter().find(|s| s.id() == id) {
                Some(s) => FrameEffect::Flat(s, e.value),
                None => panic!("{path}: unknown Warframe effect kind `{kind}`"),
            }
        }
        kind => match FrameStat::from_kind(kind) {
            Some(s) => FrameEffect::Bonus(s, e.rank_max),
            // AN UNKNOWN KIND IS REFUSED, never dropped: a card that silently
            // loses a line reads exactly like a card that has none.
            None => panic!("{path}: unknown Warframe effect kind `{kind}`"),
        },
    }
}

fn leak_all(prefix: &str) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
    crate::data::files_under(prefix)
}

/// A value at `rank` from its max-rank value: `max × (rank + 1) / (max_rank + 1)`.
///
/// No page states the rule; it is the one Umbral Vitality's own table fits at
/// every rank (9% at rank 0, 55% at rank 5, 100% at rank 10), and W`Mod` says
/// the card rounds what it prints, which is why rank 5 reads 55 and not 54.5.
pub fn at_rank(max_value: f64, rank: u32, max_rank: u32) -> f64 {
    max_value * f64::from(rank.min(max_rank) + 1) / f64::from(max_rank + 1)
}

// ---- mods -----------------------------------------------------------------

/// A card a Warframe build seats: a Warframe mod, or an aura from `data/auras/`.
#[derive(Debug, Clone)]
pub struct WarframeMod {
    pub id: String,
    pub name: String,
    pub rarity: String,
    pub polarity: String,
    /// RANK-0 drain, or the rank-0 capacity an aura grants.
    pub base_drain: u32,
    pub max_rank: u32,
    pub exilus: bool,
    /// Seated in the aura slot and nowhere else.
    pub aura: bool,
    pub family: Option<String>,
    pub set: Option<String>,
    /// (cards of the set seated, share of this card's value added).
    pub set_bonus: Vec<(u32, f64)>,
    /// The ability this augments; it pays nothing without that ability.
    pub augments: Option<String>,
    pub internal_name: Option<String>,
    pub description: String,
    pub effects: Vec<FrameEffect>,
    pub tags: Vec<TagGrant>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawMod {
    #[serde(default)]
    tags: Vec<RawTag>,
    id: String,
    name: String,
    rarity: String,
    polarity: String,
    base_drain: u32,
    max_rank: u32,
    #[serde(default)]
    exilus: bool,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    set: Option<String>,
    #[serde(default)]
    set_bonus: BTreeMap<u32, f64>,
    #[serde(default)]
    augments: Option<String>,
    #[serde(default)]
    internal_name: Option<String>,
    #[serde(default)]
    description: String,
    effects: Vec<RawEffect>,
    #[serde(default)]
    source: SourceFile,
}

fn aura_card(a: &crate::auras_data::AuraDef) -> WarframeMod {
    let mut effects = Vec::new();
    // WHAT THE FIGHT READS IS NOT THIS PANEL'S: Corrosive Projection strips
    // armour off a target, and a Warframe's own numbers do not move for it.
    if a.effect != AuraEffect::None {
        effects.push(FrameEffect::OutOfScope("the simulator's fight reads this one".into()));
    }
    effects.extend(a.out_of_scope.iter().cloned().map(FrameEffect::OutOfScope));
    WarframeMod {
        id: a.id.clone(),
        name: a.name.clone(),
        rarity: a.rarity.clone(),
        polarity: a.polarity.clone(),
        base_drain: a.base_drain,
        max_rank: a.max_rank,
        exilus: a.exilus,
        aura: !a.exilus,
        family: None,
        set: None,
        set_bonus: Vec::new(),
        augments: None,
        internal_name: a.internal_name.clone(),
        description: a.description.clone(),
        effects,
        tags: Vec::new(),
        url: None,
    }
}

/// Every card a Warframe build can seat, sorted by id.
pub fn mods() -> &'static [WarframeMod] {
    static M: OnceLock<Vec<WarframeMod>> = OnceLock::new();
    M.get_or_init(|| {
        let mut out: Vec<WarframeMod> = leak_all("warframe_mods/")
            .map(|(p, text)| {
                let r: RawMod = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                WarframeMod {
                    effects: r.effects.iter().map(|e| effect(p, e)).collect(),
                    tags: tags_of(p, &r.tags),
                    id: r.id,
                    name: r.name,
                    rarity: r.rarity,
                    polarity: r.polarity,
                    base_drain: r.base_drain,
                    max_rank: r.max_rank,
                    exilus: r.exilus,
                    aura: false,
                    family: r.family,
                    set: r.set,
                    set_bonus: r.set_bonus.into_iter().collect(),
                    augments: r.augments,
                    internal_name: r.internal_name,
                    description: r.description,
                    url: r.source.url,
                }
            })
            .collect();
        out.extend(crate::auras_data::all().iter().map(aura_card));
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    })
}

impl WarframeMod {
    /// The card at `rank`: a modelled bonus restated at that rank, every other
    /// line the card's own max-rank text.
    pub fn card_at(&self, rank: u32) -> Vec<String> {
        card_lines(&self.effects, &self.description, rank, self.max_rank)
    }
}

/// One line per effect when the card has exactly that many, else the text as
/// written. A rounded tenth is what the card prints (W`Mod`).
fn card_lines(effects: &[FrameEffect], description: &str, rank: u32, max_rank: u32) -> Vec<String> {
    let lines: Vec<&str> = description.lines().collect();
    if lines.len() != effects.len() {
        return lines.into_iter().map(str::to_string).collect();
    }
    effects
        .iter()
        .zip(lines)
        .map(|(e, line)| match e {
            FrameEffect::Bonus(s, v) => {
                let pct = (at_rank(*v, rank, max_rank) * 1000.0).round() / 10.0;
                format!("{}{}% {}", if pct >= 0.0 { "+" } else { "" }, pct, s.label())
            }
            _ => line.to_string(),
        })
        .collect()
}

pub fn mod_by_id(id: &str) -> Option<&'static WarframeMod> {
    mods().iter().find(|m| m.id == id)
}

// ---- arcanes --------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WarframeArcane {
    pub id: String,
    pub name: String,
    pub rarity: String,
    pub max_rank: u32,
    pub internal_name: Option<String>,
    pub description: String,
    pub effects: Vec<FrameEffect>,
    pub tags: Vec<TagGrant>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawArcane {
    #[serde(default)]
    tags: Vec<RawTag>,
    id: String,
    name: String,
    rarity: String,
    max_rank: u32,
    #[serde(default)]
    internal_name: Option<String>,
    #[serde(default)]
    description: String,
    effects: Vec<RawEffect>,
    #[serde(default)]
    source: SourceFile,
}

pub fn arcanes() -> &'static [WarframeArcane] {
    static A: OnceLock<Vec<WarframeArcane>> = OnceLock::new();
    A.get_or_init(|| {
        leak_all("warframe_arcanes/")
            .map(|(p, text)| {
                let r: RawArcane = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                WarframeArcane {
                    effects: r.effects.iter().map(|e| effect(p, e)).collect(),
                    tags: tags_of(p, &r.tags),
                    id: r.id,
                    name: r.name,
                    rarity: r.rarity,
                    max_rank: r.max_rank,
                    internal_name: r.internal_name,
                    description: r.description,
                    url: r.source.url,
                }
            })
            .collect()
    })
}

impl WarframeArcane {
    pub fn card_at(&self, rank: u32) -> Vec<String> {
        card_lines(&self.effects, &self.description, rank, self.max_rank)
    }
}

pub fn arcane_by_id(id: &str) -> Option<&'static WarframeArcane> {
    arcanes().iter().find(|a| a.id == id)
}

// ---- abilities ------------------------------------------------------------

/// Which build stat moves an ability's number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scaling {
    Strength,
    Duration,
    Range,
    /// A TIME divided by `1 + casting speed bonus` (W`Abilities#Casting Speed`).
    CastingSpeed,
    None,
}

#[derive(Debug, Clone)]
pub struct AbilityStat {
    pub id: String,
    pub label: String,
    /// At max rank and 100% of every stat.
    pub value: f64,
    /// `pct`, `flat`, `m`, `seconds` or `multiplier`.
    pub unit: String,
    pub scales_with: Scaling,
    /// The subsumed version's number, where the page prints a different one.
    pub helminth_value: Option<f64>,
    pub min: Option<f64>,
    pub cap: Option<f64>,
    /// A share of the frame's BASE stat, additive with that stat's mods.
    pub adds_to_base: Option<FrameStat>,
    /// (ability, multiplier) — this bonus is multiplied while that one runs.
    pub channel_multiplier: Option<(String, f64)>,
}

#[derive(Debug, Clone)]
pub struct Ability {
    pub id: String,
    pub name: String,
    /// The Warframe it comes from, or `helminth` for the Helminth's own.
    pub frame: String,
    /// 1-4 on its own frame; `None` for a Helminth ability.
    pub slot: Option<u8>,
    pub energy_cost: f64,
    /// When the cost is not energy (shields, health …), what it is.
    pub cost_type: Option<String>,
    pub subsumable: bool,
    pub augments: Vec<String>,
    pub icon: String,
    pub description: String,
    pub drain_per_second: Option<f64>,
    pub stats: Vec<AbilityStat>,
    pub unmodelled: Vec<String>,
    pub tags: Vec<TagGrant>,
    /// The infused version's cost, where its page states a different one.
    pub infused_energy_cost: Option<f64>,
    /// What the infused version does differently, verbatim.
    pub infused_notes: Vec<String>,
    pub internal_name: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawChannel {
    ability: String,
    value: f64,
}

#[derive(Debug, Deserialize)]
struct RawStat {
    id: String,
    label: String,
    value: f64,
    unit: String,
    scales_with: String,
    #[serde(default)]
    helminth_value: Option<f64>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    cap: Option<f64>,
    #[serde(default)]
    adds_to_base: Option<String>,
    #[serde(default)]
    channel_multiplier: Option<RawChannel>,
}

#[derive(Debug, Deserialize)]
struct RawAbility {
    id: String,
    name: String,
    frame: String,
    #[serde(default)]
    slot: Option<u8>,
    energy_cost: f64,
    #[serde(default)]
    cost_type: Option<String>,
    #[serde(default)]
    subsumable: bool,
    #[serde(default)]
    augments: Vec<String>,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    drain_per_second: Option<f64>,
    #[serde(default)]
    stats: Vec<RawStat>,
    #[serde(default)]
    unmodelled: Vec<String>,
    #[serde(default)]
    tags: Vec<RawTag>,
    #[serde(default)]
    infused_energy_cost: Option<f64>,
    #[serde(default)]
    infused_notes: Vec<String>,
    #[serde(default)]
    internal_name: Option<String>,
    #[serde(default)]
    source: SourceFile,
}

fn scaling(path: &str, s: &str) -> Scaling {
    match s {
        "strength" => Scaling::Strength,
        "duration" => Scaling::Duration,
        "range" => Scaling::Range,
        "casting_speed" => Scaling::CastingSpeed,
        "none" => Scaling::None,
        other => panic!("{path}: unknown scaling `{other}`"),
    }
}

pub fn abilities() -> &'static [Ability] {
    static A: OnceLock<Vec<Ability>> = OnceLock::new();
    A.get_or_init(|| {
        leak_all("warframe_abilities/")
            .map(|(p, text)| {
                let r: RawAbility = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                Ability {
                    stats: r
                        .stats
                        .iter()
                        .map(|s| AbilityStat {
                            id: s.id.clone(),
                            label: s.label.clone(),
                            value: s.value,
                            unit: s.unit.clone(),
                            scales_with: scaling(p, &s.scales_with),
                            helminth_value: s.helminth_value,
                            min: s.min,
                            cap: s.cap,
                            adds_to_base: s.adds_to_base.as_deref().map(FrameStat::parse),
                            channel_multiplier: s
                                .channel_multiplier
                                .as_ref()
                                .map(|c| (c.ability.clone(), c.value)),
                        })
                        .collect(),
                    id: r.id,
                    name: r.name,
                    frame: r.frame,
                    slot: r.slot,
                    energy_cost: r.energy_cost,
                    cost_type: r.cost_type,
                    subsumable: r.subsumable,
                    augments: r.augments,
                    icon: r.icon,
                    description: r.description,
                    drain_per_second: r.drain_per_second,
                    tags: tags_of(p, &r.tags),
                    infused_energy_cost: r.infused_energy_cost,
                    infused_notes: r.infused_notes,
                    unmodelled: r.unmodelled,
                    internal_name: r.internal_name,
                    url: r.source.url,
                }
            })
            .collect()
    })
}

pub fn ability(id: &str) -> Option<&'static Ability> {
    abilities().iter().find(|a| a.id == id)
}

/// Every ability the Helminth can infuse.
pub fn helminth_pool() -> impl Iterator<Item = &'static Ability> {
    abilities().iter().filter(|a| a.subsumable)
}

// ---- frames ---------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WarframeDef {
    pub id: String,
    pub name: String,
    pub internal_name: Option<String>,
    /// Rank 30.
    pub health: f64,
    pub shield: f64,
    /// `data/frames.yaml`'s, which the fight's Tenno reads too.
    pub armor: f64,
    pub energy: f64,
    pub sprint: f64,
    /// Innate polarities, positions unpublished.
    pub polarities: Vec<String>,
    pub aura_polarity: Option<String>,
    pub exilus_polarity: Option<String>,
    pub passive: String,
    /// What the passive grants, as tags.
    pub passive_tags: Vec<TagGrant>,
    /// Ability ids in slot order.
    pub abilities: Vec<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawFrame {
    id: String,
    name: String,
    #[serde(default)]
    internal_name: Option<String>,
    health: f64,
    shield: f64,
    #[serde(default)]
    polarities: Vec<String>,
    #[serde(default)]
    aura_polarity: Option<String>,
    #[serde(default)]
    exilus_polarity: Option<String>,
    #[serde(default)]
    passive: String,
    #[serde(default)]
    passive_tags: Vec<RawTag>,
    abilities: Vec<String>,
    #[serde(default)]
    source: SourceFile,
}

pub fn warframes() -> &'static [WarframeDef] {
    static F: OnceLock<Vec<WarframeDef>> = OnceLock::new();
    F.get_or_init(|| {
        leak_all("warframes/")
            .map(|(p, text)| {
                let r: RawFrame = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                let roster = crate::tenno_data::frame(&r.id)
                    .unwrap_or_else(|| panic!("{p}: `{}` is not in data/frames.yaml", r.id));
                WarframeDef {
                    id: r.id,
                    name: r.name,
                    internal_name: r.internal_name,
                    health: r.health,
                    shield: r.shield,
                    armor: roster.armor,
                    energy: roster.energy,
                    sprint: roster.sprint,
                    polarities: r.polarities,
                    aura_polarity: r.aura_polarity,
                    exilus_polarity: r.exilus_polarity,
                    passive_tags: tags_of(p, &r.passive_tags),
                    passive: r.passive,
                    abilities: r.abilities,
                    url: r.source.url,
                }
            })
            .collect()
    })
}

pub fn warframe(id: &str) -> Option<&'static WarframeDef> {
    warframes().iter().find(|f| f.id == id)
}

// ---- a build --------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SlotPick {
    pub id: String,
    /// `None` = max rank.
    #[serde(default)]
    pub rank: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HelminthPick {
    /// 1-4.
    pub slot: u8,
    pub ability: String,
}

/// THE OPERATOR a Warframe build links: the active Focus school, which of its
/// conditional nodes to count as running, and the school's artifact. A node's
/// condition is the Operator's own action, so it is ASSUMED when ticked and
/// never simulated.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OperatorPick {
    pub school: String,
    #[serde(default)]
    pub assumed: Vec<String>,
    #[serde(default)]
    pub artifact: ArtifactPick,
}

/// The active school's artifact, every card at max rank.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ArtifactPick {
    #[serde(default)]
    pub mods: Vec<String>,
    #[serde(default)]
    pub arcane: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Build {
    #[serde(default)]
    pub operator: Option<OperatorPick>,
    pub frame: String,
    #[serde(default)]
    pub mods: Vec<SlotPick>,
    #[serde(default)]
    pub exilus: Option<SlotPick>,
    #[serde(default)]
    pub aura: Option<SlotPick>,
    #[serde(default)]
    pub arcanes: Vec<SlotPick>,
    #[serde(default)]
    pub shards: Vec<ShardPick>,
    #[serde(default)]
    pub helminth: Option<HelminthPick>,
}

/// One source's share of a stat.
#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
    /// The mod, arcane or shard effect id (`<shard>/<effect>`).
    pub from: String,
    pub value: f64,
    /// A flat amount added after the multiplier, rather than a percentage.
    pub flat: bool,
    /// A multiplier on the finished stat (Catalyzing Shields' x0.20).
    pub times: bool,
}

/// THE SHIELD GATE: how long "invulnerable when shields break" lasts on this
/// build, and whether casting re-opens it. docs/WARFRAMES.md §Shield gate.
#[derive(Debug, Clone, PartialEq)]
pub struct ShieldGate {
    pub max_shields: f64,
    /// The gate after a FULL break, in seconds.
    pub full_seconds: f64,
    /// The card that fixes it, when one does.
    pub fixed_by: Option<String>,
    /// Shields per energy spent casting, and each source's share.
    pub energy_to_shield: f64,
    pub sources: Vec<(String, f64)>,
    pub casts: Vec<GateCast>,
}

/// One ability's cast, as the shield gate sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct GateCast {
    pub slot: u8,
    pub ability: String,
    pub energy: f64,
    pub shields: f64,
    /// The refill reaches max shields.
    pub full: bool,
    pub seconds: f64,
}

/// The gate after shields break holding `s` shields (W`Shield`):
/// "Shield/180 + 1/3" under 53, "(Shield/350)^0.65 + 1/3" to 1,150, 2.5 above.
pub fn shield_gate_seconds(s: f64) -> f64 {
    if s <= 0.0 {
        0.0
    } else if s < 53.0 {
        s / 180.0 + 1.0 / 3.0
    } else if s <= 1150.0 {
        (s / 350.0).powf(0.65) + 1.0 / 3.0
    } else {
        2.5
    }
}

/// A set's energy-to-shield share for this many cards seated, from
/// `data/mod_sets/<set>.yaml`'s `energy_to_shield_by_count`.
fn set_energy_to_shield(set: &str, count: u32) -> f64 {
    #[derive(Deserialize)]
    struct SetShield {
        id: String,
        #[serde(default)]
        energy_to_shield_by_count: BTreeMap<u32, f64>,
    }
    crate::data::files_under("mod_sets/")
        .filter_map(|(_, t)| serde_norway::from_str::<SetShield>(t).ok())
        .find(|s| s.id == set)
        .and_then(|s| s.energy_to_shield_by_count.get(&count).copied())
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatLine {
    pub stat: FrameStat,
    pub base: f64,
    /// Σ percentage bonuses.
    pub bonus: f64,
    /// Σ flat bonuses.
    pub flat: f64,
    pub value: f64,
    pub sources: Vec<Contribution>,
}

#[derive(Debug, Clone)]
pub struct AbilityLine {
    pub stat: &'static AbilityStat,
    /// Before the build's stats, after the Helminth's own number.
    pub base: f64,
    pub value: f64,
}

/// A number an ability produces from the frame's own stats.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedLine {
    pub label: String,
    pub stat: FrameStat,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct ResolvedAbility {
    pub slot: u8,
    pub ability: &'static Ability,
    pub helminth: bool,
    pub energy_cost: f64,
    pub drain_per_second: Option<f64>,
    pub lines: Vec<AbilityLine>,
    pub derived: Vec<DerivedLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionKind {
    /// Not computed yet.
    Unmodelled,
    /// Cannot pay out in this panel.
    OutOfScope,
    /// Seated, and pays nothing in this build.
    Inert,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Admission {
    pub from: String,
    pub text: String,
    pub kind: AdmissionKind,
}

/// One tag the build carries, and what carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct TagSource {
    pub tag: Capability,
    /// The mod, arcane or ability id, or the frame's id for its passive.
    pub from: String,
    pub when: String,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub frame: &'static WarframeDef,
    pub stats: Vec<StatLine>,
    pub abilities: Vec<ResolvedAbility>,
    /// Every tag and every source of it, in tag order. An augment without its
    /// ability grants nothing, as it pays nothing.
    pub tags: Vec<TagSource>,
    pub shield_gate: ShieldGate,
    pub admissions: Vec<Admission>,
    /// What the build asked for and could not seat, each with the reason.
    pub refused: Vec<String>,
}

fn by_rank(ladder: &[f64], rank: u32) -> f64 {
    ladder[(rank as usize).min(ladder.len() - 1)]
}

impl Resolved {
    pub fn stat(&self, s: FrameStat) -> &StatLine {
        self.stats.iter().find(|l| l.stat == s).expect("every stat is resolved")
    }
}

/// What a socket is worth to a Warframe's own panel: (stat, value, is it flat).
///
/// Keyed by the shard EFFECT id, because that is what names the quantity. Every
/// value here is transcribed in `data/shards/`, and W`Archon_Shard` says how it
/// lands: the Crimson and Amber percentages are "additive with similar buffs",
/// and each Azure number is a "Flat value increase after all bonuses are applied".
fn shard_stat(effect: &str) -> Option<(FrameStat, bool)> {
    Some(match effect {
        "ability_strength" => (FrameStat::AbilityStrength, false),
        "ability_duration" => (FrameStat::AbilityDuration, false),
        "casting_speed" => (FrameStat::CastingSpeed, false),
        "max_health" => (FrameStat::Health, true),
        "shield_capacity" => (FrameStat::Shield, true),
        "armor" => (FrameStat::Armor, true),
        "energy_max" => (FrameStat::Energy, true),
        _ => return None,
    })
}

/// Resolve a build. An `Err` is a build that names no frame this data has;
/// everything else the build cannot seat is `refused`, and the rest resolves.
pub fn resolve(b: &Build) -> Result<Resolved, String> {
    let frame = warframe(&b.frame).ok_or_else(|| format!("unknown Warframe: {}", b.frame))?;
    let mut refused: Vec<String> = Vec::new();
    let mut admissions: Vec<Admission> = Vec::new();

    // THE LOADOUT FIRST, because an augment pays only while its ability is in it.
    let mut loadout: Vec<(u8, &'static Ability, bool)> = frame
        .abilities
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let a = ability(id).unwrap_or_else(|| panic!("{}: unknown ability {id}", frame.id));
            (i as u8 + 1, a, false)
        })
        .collect();
    if let Some(h) = &b.helminth {
        match ability(&h.ability) {
            Some(a) if !a.subsumable => refused.push(format!("{} cannot be infused", a.name)),
            Some(a) if frame.abilities.contains(&a.id) => {
                refused.push(format!("{} already has {}", frame.name, a.name))
            }
            Some(a) if (1..=4).contains(&h.slot) => loadout[h.slot as usize - 1] = (h.slot, a, true),
            Some(_) => refused.push(format!("there is no ability slot {}", h.slot)),
            None => refused.push(format!("unknown ability: {}", h.ability)),
        }
    }
    let carries = |id: &str| loadout.iter().any(|(_, a, _)| a.id == id);

    // THE CARDS, each checked against the slot it sits in.
    let mut seated: Vec<(&'static WarframeMod, u32)> = Vec::new();
    let mut seat = |pick: &SlotPick, slot: &str, refused: &mut Vec<String>| {
        let Some(m) = mod_by_id(&pick.id) else {
            refused.push(format!("unknown mod: {}", pick.id));
            return;
        };
        let why = match slot {
            "aura" if !m.aura => Some("is not an aura"),
            "exilus" if !m.exilus => Some("is not an exilus mod"),
            "main" if m.aura => Some("goes in the aura slot"),
            _ => None,
        };
        if let Some(w) = why {
            refused.push(format!("{} {w}", m.name));
            return;
        }
        if seated.iter().any(|(x, _)| x.id == m.id) {
            refused.push(format!("{} is already seated", m.name));
            return;
        }
        if let Some(f) = &m.family {
            if let Some((x, _)) = seated.iter().find(|(x, _)| x.family.as_ref() == Some(f)) {
                refused.push(format!("{} cannot be seated beside {}", m.name, x.name));
                return;
            }
        }
        seated.push((m, pick.rank.unwrap_or(m.max_rank).min(m.max_rank)));
    };
    for p in b.mods.iter().take(8) {
        seat(p, "main", &mut refused);
    }
    if b.mods.len() > 8 {
        refused.push(format!("{} mods for eight slots", b.mods.len()));
    }
    if let Some(p) = &b.exilus {
        seat(p, "exilus", &mut refused);
    }
    if let Some(p) = &b.aura {
        seat(p, "aura", &mut refused);
    }

    let mut bonus: BTreeMap<FrameStat, (f64, f64, Vec<Contribution>)> = BTreeMap::new();
    let mut add = |stat: FrameStat, from: &str, value: f64, flat: bool| {
        let e = bonus.entry(stat).or_default();
        if flat {
            e.1 += value;
        } else {
            e.0 += value;
        }
        e.2.push(Contribution { from: from.to_string(), value, flat, times: false });
    };
    // The two shield-gate cards: a multiplier on the finished shields, and a
    // fixed gate length.
    let mut shield_times: Vec<(String, f64)> = Vec::new();
    let mut gate_fixed: Option<(String, f64)> = None;

    for (m, rank) in &seated {
        // A SET BONUS counts the set's cards seated, and adds a share of THIS
        // card's own value.
        let n = m
            .set
            .as_ref()
            .map_or(0, |s| seated.iter().filter(|(x, _)| x.set.as_ref() == Some(s)).count() as u32);
        let set_share = m.set_bonus.iter().filter(|(k, _)| *k <= n).map(|(_, v)| *v).next_back().unwrap_or(0.0);
        let dead = m.augments.as_deref().filter(|a| !carries(a));
        if let Some(a) = dead {
            admissions.push(Admission {
                from: m.id.clone(),
                text: format!("augments {}, which this loadout does not carry", ability(a).map_or(a, |x| x.name.as_str())),
                kind: AdmissionKind::Inert,
            });
        }
        for e in &m.effects {
            match e {
                FrameEffect::Bonus(s, v) if dead.is_none() => {
                    add(*s, &m.id, at_rank(*v, *rank, m.max_rank) * (1.0 + set_share), false)
                }
                FrameEffect::Bonus(..) => {}
                FrameEffect::Flat(s, v) if dead.is_none() => add(*s, &m.id, *v, true),
                FrameEffect::Flat(..) => {}
                FrameEffect::ShieldMultiplier(r) => shield_times.push((m.id.clone(), by_rank(r, *rank))),
                FrameEffect::ShieldGateSeconds(r) => gate_fixed = Some((m.id.clone(), by_rank(r, *rank))),
                FrameEffect::Unmodelled(t) => admissions.push(Admission {
                    from: m.id.clone(),
                    text: t.clone(),
                    kind: AdmissionKind::Unmodelled,
                }),
                FrameEffect::OutOfScope(t) => admissions.push(Admission {
                    from: m.id.clone(),
                    text: t.clone(),
                    kind: AdmissionKind::OutOfScope,
                }),
            }
        }
    }

    let mut arcane_ids: Vec<&str> = Vec::new();
    for p in b.arcanes.iter().take(2) {
        let Some(a) = arcane_by_id(&p.id) else {
            refused.push(format!("unknown arcane: {}", p.id));
            continue;
        };
        if arcane_ids.contains(&a.id.as_str()) {
            refused.push(format!("{} is already seated", a.name));
            continue;
        }
        arcane_ids.push(&a.id);
        let rank = p.rank.unwrap_or(a.max_rank).min(a.max_rank);
        for e in &a.effects {
            match e {
                FrameEffect::Bonus(s, v) => add(*s, &a.id, at_rank(*v, rank, a.max_rank), false),
                FrameEffect::Flat(s, v) => add(*s, &a.id, *v, true),
                FrameEffect::ShieldMultiplier(r) => shield_times.push((a.id.clone(), by_rank(r, rank))),
                FrameEffect::ShieldGateSeconds(r) => gate_fixed = Some((a.id.clone(), by_rank(r, rank))),
                FrameEffect::Unmodelled(t) => admissions.push(Admission {
                    from: a.id.clone(),
                    text: t.clone(),
                    kind: AdmissionKind::Unmodelled,
                }),
                FrameEffect::OutOfScope(t) => admissions.push(Admission {
                    from: a.id.clone(),
                    text: t.clone(),
                    kind: AdmissionKind::OutOfScope,
                }),
            }
        }
    }

    for pick in b.shards.iter().take(5) {
        let Some(d) = crate::shards_data::all().iter().find(|s| s.id == pick.shard) else {
            refused.push(format!("unknown shard: {}", pick.shard));
            continue;
        };
        let Some(o) = d.options.iter().find(|o| o.id == pick.effect) else {
            refused.push(format!("{} has no effect {}", d.name, pick.effect));
            continue;
        };
        let from = format!("{}/{}", d.id, o.id);
        let v = if pick.tauforged { o.tauforged } else { o.value };
        match shard_stat(&o.id) {
            Some((s, flat)) => add(s, &from, v, flat),
            None => admissions.push(Admission {
                from,
                text: o.text.clone(),
                kind: AdmissionKind::OutOfScope,
            }),
        }
    }

    // THE OPERATOR'S FOCUS: an always-on node counts, a conditional one only
    // when the Operator build assumes it.
    let school = match &b.operator {
        Some(o) => match focus_school(&o.school) {
            Some(s) => Some((s, o)),
            None => {
                refused.push(format!("unknown Focus school: {}", o.school));
                None
            }
        },
        None => None,
    };
    if let Some((_, o)) = school {
        refused.extend(artifact_refusals(&o.artifact));
    }
    if let Some((s, o)) = school {
        for n in s.nodes.iter().filter(|n| n.always || o.assumed.contains(&n.id)) {
            let from = format!("focus:{}:{}", s.id, n.id);
            for e in &n.effects {
                match e {
                    FrameEffect::Bonus(st, v) => add(*st, &from, *v, false),
                    FrameEffect::Flat(st, v) => add(*st, &from, *v, true),
                    FrameEffect::ShieldMultiplier(r) => shield_times.push((from.clone(), by_rank(r, 99))),
                    FrameEffect::ShieldGateSeconds(r) => gate_fixed = Some((from.clone(), by_rank(r, 99))),
                    FrameEffect::Unmodelled(_) | FrameEffect::OutOfScope(_) => {}
                }
            }
        }
    }

    // THE STATS. Health, shields, armour and energy are
    //   base × (1 + Σ mods) + Σ flat
    // (W`Health`, W`Shield`, W`Armor`, W`Energy_Capacity`); the ability stats
    // start at 100% and every modifier "combine[s] additively".
    let stats: Vec<StatLine> = FrameStat::ALL
        .into_iter()
        .map(|s| {
            let base = match s {
                FrameStat::Health => frame.health,
                FrameStat::Shield => frame.shield,
                FrameStat::Armor => frame.armor,
                FrameStat::Energy => frame.energy,
                FrameStat::SprintSpeed => frame.sprint,
                _ => 1.0,
            };
            let (pct, flat, mut sources) = bonus.get(&s).cloned().unwrap_or_default();
            let mut value = base * (1.0 + pct) + flat;
            // "x0.20 Max Shield Capacity" — applied to the finished shields.
            if s == FrameStat::Shield {
                for (from, t) in &shield_times {
                    value *= t;
                    sources.push(Contribution { from: from.clone(), value: *t, flat: false, times: true });
                }
            }
            StatLine { stat: s, base, bonus: pct, flat, value, sources }
        })
        .collect();
    let val = |s: FrameStat| stats.iter().find(|l| l.stat == s).map_or(1.0, |l| l.value);
    let strength = val(FrameStat::AbilityStrength);
    let duration = val(FrameStat::AbilityDuration);
    let range = val(FrameStat::AbilityRange);
    let efficiency = val(FrameStat::AbilityEfficiency);
    let casting = val(FrameStat::CastingSpeed);

    // W`Ability_Efficiency`:
    //   "Final Ability Cost = Ability Cost · max(200% − Ability Efficiency, 25%)"
    //   "Final Energy Drain = Energy Drain · max((200% − Ability Efficiency) / Ability Duration, 25%)"
    let cost_multiplier = (2.0 - efficiency).max(0.25);
    let drain_multiplier = if duration > 0.0 { ((2.0 - efficiency) / duration).max(0.25) } else { f64::INFINITY };

    let abilities: Vec<ResolvedAbility> = loadout
        .iter()
        .map(|&(slot, a, helminth)| {
            let lines: Vec<AbilityLine> = a
                .stats
                .iter()
                .map(|st| {
                    let base = if helminth { st.helminth_value.unwrap_or(st.value) } else { st.value };
                    let mut value = match st.scales_with {
                        Scaling::Strength => base * strength,
                        Scaling::Duration => base * duration,
                        Scaling::Range => base * range,
                        Scaling::CastingSpeed if casting > 0.0 => base / casting,
                        Scaling::CastingSpeed => f64::INFINITY,
                        Scaling::None => base,
                    };
                    if let Some(m) = st.min {
                        value = value.max(m);
                    }
                    if let Some(c) = st.cap {
                        value = value.min(c);
                    }
                    AbilityLine { stat: st, base, value }
                })
                .collect();
            let mut derived = Vec::new();
            for l in &lines {
                let Some(target) = l.stat.adds_to_base else { continue };
                let t = stats.iter().find(|x| x.stat == target).expect("every stat is resolved");
                derived.push(DerivedLine {
                    label: format!("{} while {} is active", target.label(), a.name),
                    stat: target,
                    value: t.base * (1.0 + t.bonus + l.value) + t.flat,
                });
                if let Some((with, mult)) = &l.stat.channel_multiplier {
                    if let Some((_, w, _)) = loadout.iter().find(|(_, x, _)| &x.id == with) {
                        derived.push(DerivedLine {
                            label: format!("{} while {} and {} are active", target.label(), a.name, w.name),
                            stat: target,
                            value: t.base * (1.0 + t.bonus + mult * l.value) + t.flat,
                        });
                    }
                }
            }
            for t in &a.unmodelled {
                admissions.push(Admission { from: a.id.clone(), text: t.clone(), kind: AdmissionKind::Unmodelled });
            }
            ResolvedAbility {
                slot,
                ability: a,
                helminth,
                energy_cost: if helminth { a.infused_energy_cost.unwrap_or(a.energy_cost) } else { a.energy_cost }
                    * cost_multiplier,
                drain_per_second: a.drain_per_second.map(|d| d * drain_multiplier),
                lines,
                derived,
            }
        })
        .collect();

    let mut tags: Vec<TagSource> = Vec::new();
    let mut claim = |from: &str, grants: &[TagGrant], infused: bool| {
        for g in grants {
            let when = match (&g.infused, infused) {
                (Some(None), true) => continue,
                (Some(Some(w)), true) => w.clone(),
                _ => g.when.clone(),
            };
            tags.push(TagSource { tag: g.tag, from: from.to_string(), when });
        }
    };
    claim(&frame.id, &frame.passive_tags, false);
    for (m, _) in &seated {
        if m.augments.as_deref().is_none_or(|a| loadout.iter().any(|(_, x, _)| x.id == a)) {
            claim(&m.id, &m.tags, false);
        }
    }
    for id in &arcane_ids {
        if let Some(a) = arcane_by_id(id) {
            claim(&a.id, &a.tags, false);
        }
    }
    for (_, a, helminth) in &loadout {
        claim(&a.id, &a.tags, *helminth);
    }
    // AN UNLOCKED NODE CAN BE USED whether or not its stat is assumed, so every
    // tag of the active school counts.
    if let Some((s, _)) = school {
        for n in &s.nodes {
            claim(&format!("focus:{}:{}", s.id, n.id), &n.tags, false);
        }
    }
    // THE SHIELD GATE. Energy spent casting is converted to shields by the Augur
    // set and Brief Respite, and any refill re-opens the gate. Its length is the
    // shields refilled — or Catalyzing Shields' fixed value, whatever the refill
    // (MEASUREMENTS M92).
    let max_shields = stats.iter().find(|l| l.stat == FrameStat::Shield).map_or(0.0, |l| l.value);
    let mut sources: Vec<(String, f64)> = Vec::new();
    let augur = seated.iter().filter(|(m, _)| m.set.as_deref() == Some("augur")).count() as u32;
    if augur > 0 {
        sources.push(("augur".into(), set_energy_to_shield("augur", augur)));
    }
    for (m, rank) in seated.iter().filter(|(m, _)| m.aura) {
        if let Some(v) = crate::auras_data::by_id(&m.id).and_then(|a| a.energy_to_shield) {
            sources.push((m.id.clone(), at_rank(v, *rank, m.max_rank)));
        }
    }
    let energy_to_shield: f64 = sources.iter().map(|(_, v)| v).sum();
    let casts: Vec<GateCast> = if max_shields > 0.0 && energy_to_shield > 0.0 {
        abilities
            .iter()
            .filter(|x| x.ability.cost_type.is_none() && x.energy_cost > 0.0)
            .map(|x| {
                let shields = x.energy_cost * energy_to_shield;
                let full = shields >= max_shields;
                let seconds = match &gate_fixed {
                    Some((_, fixed)) => *fixed,
                    None => shield_gate_seconds(shields.min(max_shields)),
                };
                GateCast { slot: x.slot, ability: x.ability.id.clone(), energy: x.energy_cost, shields, full, seconds }
            })
            .collect()
    } else {
        Vec::new()
    };
    if !casts.is_empty() {
        tags.push(TagSource {
            tag: Capability::Invulnerable,
            from: "shield_gate".into(),
            when: format!(
                "the shield gate, re-opened by casting: {}",
                casts
                    .iter()
                    .map(|c| format!("{} {:.2} s", ability(&c.ability).map_or(c.ability.as_str(), |a| a.name.as_str()), c.seconds))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    let shield_gate = ShieldGate {
        max_shields,
        full_seconds: gate_fixed.as_ref().map_or(shield_gate_seconds(max_shields), |(_, s)| *s),
        fixed_by: gate_fixed.map(|(id, _)| id),
        energy_to_shield,
        sources,
        casts,
    };
    tags.sort_by_key(|t| t.tag);

    Ok(Resolved { frame, stats, abilities, tags, shield_gate, admissions, refused })
}

/// The capacity an aura adds, from W`Aura`: "matching polarity … double of the
/// Aura's "drain" … In a slot without a polarity, the capacity is the same as the
/// listed drain, and in a slot of a different polarity, the additional capacity is
/// 80% of listed drain, rounded down". W`Mod` says "reduces it by 25%, rounded
/// mathematically" instead; the two part only at 2, 6, 10 and 14. The stance
/// slot (`mods::stance_capacity`) already follows the Aura page.
pub fn aura_capacity(grant: u32, aura_polarity: &str, slot_polarity: Option<&str>) -> u32 {
    match slot_polarity {
        None => grant,
        Some(p) if p == aura_polarity || p == "omni" => grant * 2,
        Some(_) => (f64::from(grant) * 0.8).floor() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(mods: &[&str]) -> Build {
        Build {
            frame: "valkyr".into(),
            mods: mods.iter().map(|m| SlotPick { id: (*m).into(), rank: None }).collect(),
            ..Build::default()
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn every_file_loads_and_names_what_exists() {
        assert!(mods().len() > 100, "{}", mods().len());
        assert!(arcanes().len() > 50);
        let mut ids: Vec<&str> = mods().iter().map(|m| m.id.as_str()).collect();
        ids.dedup();
        assert_eq!(ids.len(), mods().len(), "a mod id is filed twice");
        for m in mods() {
            if let Some(a) = &m.augments {
                assert!(ability(a).is_some(), "{} augments `{a}`, which is not in data/warframe_abilities/", m.id);
            }
            if let Some(f) = &m.family {
                assert!(mods().iter().filter(|x| x.family.as_ref() == Some(f)).count() > 1, "{}: a family of one", m.id);
            }
        }
        for a in abilities() {
            assert!(!a.icon.is_empty(), "{} has no icon", a.id);
            for g in &a.augments {
                assert!(mod_by_id(g).is_some() || a.frame != "valkyr", "{} names augment {g}", a.id);
            }
        }
        for f in warframes() {
            assert_eq!(f.abilities.len(), 4, "{}", f.id);
            for id in &f.abilities {
                assert!(ability(id).is_some(), "{}: {id}", f.id);
            }
        }
        assert!(helminth_pool().count() > 70);
    }

    /// A MODELLED LINE RESTATED AT MAX RANK IS THE CARD ITSELF. The effect list
    /// was read off each card, so a restatement that disagrees is a line the
    /// model reads differently from the card.
    #[test]
    fn every_card_restates_itself_at_max_rank() {
        for m in mods() {
            assert_eq!(m.card_at(m.max_rank).join("\n"), m.description, "{}", m.id);
        }
        for a in arcanes() {
            assert_eq!(a.card_at(a.max_rank).join("\n"), a.description, "{}", a.id);
        }
        assert_eq!(mod_by_id("intensify").unwrap().card_at(0), vec!["+5% Ability Strength"]);
    }

    #[test]
    fn a_bare_valkyr_is_her_rank_30_arsenal() {
        let r = resolve(&build(&[])).unwrap();
        assert_eq!(r.stat(FrameStat::Health).value, 750.0);
        assert_eq!(r.stat(FrameStat::Shield).value, 185.0);
        assert_eq!(r.stat(FrameStat::Armor).value, 855.0);
        assert_eq!(r.stat(FrameStat::Energy).value, 150.0);
        assert_eq!(r.stat(FrameStat::AbilityStrength).value, 1.0);
        assert_eq!(r.abilities.len(), 4);
        assert_eq!(r.abilities[1].ability.id, "warcry");
        assert_eq!(r.abilities[1].energy_cost, 75.0);
    }

    /// Umbral Vitality's table (W`Umbral_Vitality`): 9% at rank 0, 55% at rank 5.
    #[test]
    fn a_mod_scales_by_rank_plus_one() {
        assert!(close(at_rank(1.0, 0, 10), 1.0 / 11.0));
        assert_eq!((at_rank(1.0, 5, 10) * 100.0).round(), 55.0);
        let mut b = build(&["intensify"]);
        b.mods[0].rank = Some(0);
        assert!(close(resolve(&b).unwrap().stat(FrameStat::AbilityStrength).value, 1.05));
    }

    /// W`Umbral_Set`: Vitality 100% → 180% and Intensify 44% → 77% with all three.
    #[test]
    fn the_umbral_set_raises_each_card_by_its_own_share() {
        let r = resolve(&build(&["umbral_vitality", "umbral_fiber", "umbral_intensify"])).unwrap();
        assert!(close(r.stat(FrameStat::Health).bonus, 1.8));
        assert!(close(r.stat(FrameStat::Armor).bonus, 1.8));
        assert!(close(r.stat(FrameStat::AbilityStrength).bonus, 0.77));
        let two = resolve(&build(&["umbral_vitality", "umbral_intensify"])).unwrap();
        assert!(close(two.stat(FrameStat::Health).bonus, 1.3));
        assert!(close(two.stat(FrameStat::AbilityStrength).bonus, 0.55));
    }

    /// W`Warcry`: "855 × (1 + 1 + 0.5 × (1 + 0.3))" for Steel Fiber and Intensify.
    #[test]
    fn warcry_adds_to_base_armor_and_triples_in_hysteria() {
        let r = resolve(&build(&["steel_fiber", "intensify"])).unwrap();
        let w = &r.abilities[1];
        assert_eq!(w.derived.len(), 2);
        assert!(close(w.derived[0].value, 855.0 * (1.0 + 1.0 + 0.5 * 1.3)));
        assert!(close(w.derived[1].value, 855.0 * (1.0 + 1.0 + 3.0 * 0.5 * 1.3)));
    }

    /// W`Ability_Efficiency` and W`Energy_Capacity`'s worked example.
    #[test]
    fn cost_and_drain_follow_the_efficiency_page() {
        // Fleeting Expertise: +60% efficiency, -60% duration.
        let r = resolve(&build(&["fleeting_expertise", "streamline"])).unwrap();
        let eff = r.stat(FrameStat::AbilityEfficiency).value;
        assert!(close(eff, 1.9));
        assert!(close(r.abilities[1].energy_cost, 75.0 * 0.25), "the 25% floor");
        let h = &r.abilities[3];
        let dur = r.stat(FrameStat::AbilityDuration).value;
        assert!(close(h.drain_per_second.unwrap(), 5.0 * ((2.0 - eff) / dur).max(0.25)));
        // "300 × (1+1.85+0.25) + 75 = 1005" — here on Valkyr's 150.
        let mut b = build(&["primed_flow"]);
        b.shards.push(ShardPick { shard: "azure_archon_shard".into(), effect: "energy_max".into(), tauforged: true });
        assert!(close(resolve(&b).unwrap().stat(FrameStat::Energy).value, 150.0 * 2.85 + 75.0));
    }

    #[test]
    fn the_helminth_replaces_one_slot_with_its_own_numbers() {
        let mut b = build(&[]);
        b.helminth = Some(HelminthPick { slot: 1, ability: "roar".into() });
        let r = resolve(&b).unwrap();
        assert_eq!(r.abilities[0].ability.id, "roar");
        assert!(r.abilities[0].helminth);
        assert!(r.refused.is_empty());
        // Her own ability is not a second copy.
        b.helminth = Some(HelminthPick { slot: 1, ability: "warcry".into() });
        assert_eq!(resolve(&b).unwrap().refused.len(), 1);
        let as_infused = ability("warcry").unwrap().stats[0].helminth_value;
        assert_eq!(as_infused, Some(0.3));
        // "Subsumed Pillage uses 50 energy instead of shields."
        b.helminth = Some(HelminthPick { slot: 1, ability: "pillage".into() });
        let r = resolve(&b).unwrap();
        assert_eq!(r.abilities[0].energy_cost, 50.0);
        assert!(!r.abilities[0].ability.infused_notes.is_empty());
    }

    #[test]
    fn an_augment_without_its_ability_pays_nothing_and_says_so() {
        let mut b = build(&["eternal_war"]);
        assert!(resolve(&b).unwrap().admissions.iter().all(|a| a.kind != AdmissionKind::Inert));
        b.helminth = Some(HelminthPick { slot: 2, ability: "roar".into() });
        assert!(resolve(&b).unwrap().admissions.iter().any(|a| a.kind == AdmissionKind::Inert));
    }

    #[test]
    fn a_slot_refuses_what_does_not_belong_in_it() {
        let mut b = build(&["intensify", "umbral_intensify", "steel_charge"]);
        b.exilus = Some(SlotPick { id: "intensify".into(), rank: None });
        let r = resolve(&b).unwrap();
        // a family twice, an aura in a main slot, a non-exilus in the exilus slot
        assert_eq!(r.refused.len(), 3, "{:?}", r.refused);
    }

    fn tags_of_build(b: &Build) -> Vec<(Capability, String)> {
        resolve(b).unwrap().tags.into_iter().map(|t| (t.tag, t.from)).collect()
    }

    /// Rolling Guard: "grants a brief period of invulnerability and removes all
    /// Status Effects when rolling"; Valkyr's passive grants invulnerability.
    #[test]
    fn a_tag_names_every_source_that_grants_it() {
        let b = build(&["rolling_guard"]);
        let t = tags_of_build(&b);
        assert!(t.contains(&(Capability::Invulnerable, "valkyr".into())));
        assert!(t.contains(&(Capability::Invulnerable, "rolling_guard".into())));
        assert!(t.contains(&(Capability::StatusCleanse, "rolling_guard".into())));
        assert!(!t.iter().any(|(c, _)| *c == Capability::DamageCap));
        // Hysteria: "becoming immune to Status Effects" — an immunity, not a cleanse.
        assert!(t.contains(&(Capability::StatusImmunity, "hysteria".into())));
        assert!(!t.contains(&(Capability::StatusCleanse, "hysteria".into())));
    }

    /// "Subsumed Omamori ... cannot gain invulnerability", and Well of Life's
    /// infused cooldown is 120 s.
    #[test]
    fn an_infused_ability_carries_the_infused_versions_tags() {
        let mut b = build(&[]);
        b.helminth = Some(HelminthPick { slot: 1, ability: "omamori".into() });
        assert!(!tags_of_build(&b).iter().any(|(_, f)| f == "omamori"));
        b.helminth = Some(HelminthPick { slot: 1, ability: "well_of_life".into() });
        let r = resolve(&b).unwrap();
        let w = r.tags.iter().find(|t| t.from == "well_of_life").expect("the infused Well of Life");
        assert!(w.when.contains("120 s"), "{}", w.when);
    }

    /// An always-on node counts, a conditional one only when assumed, and the
    /// active school's tags count either way.
    #[test]
    fn the_operator_counts_what_is_always_on_and_what_is_assumed() {
        let mut b = build(&[]);
        b.operator = Some(OperatorPick { school: "unairu".into(), assumed: vec![], ..Default::default() });
        let r = resolve(&b).unwrap();
        assert_eq!(r.stat(FrameStat::Armor).value, 855.0 + 200.0, "Stone Skin");
        assert!(r.tags.iter().any(|t| t.from == "focus:unairu:reinforced_return"));
        b.operator = Some(OperatorPick { school: "madurai".into(), assumed: vec![], ..Default::default() });
        assert_eq!(resolve(&b).unwrap().stat(FrameStat::AbilityStrength).value, 1.0);
        b.operator = Some(OperatorPick {
            school: "madurai".into(),
            assumed: vec!["sling_strength".into()],
            ..Default::default()
        });
        assert!(close(resolve(&b).unwrap().stat(FrameStat::AbilityStrength).value, 1.4));
    }

    /// Every school has its artifact, every card's school and `bonus_per` name a
    /// school, and a seating the artifact cannot hold is refused.
    #[test]
    fn the_artifact_seats_five_known_mods_once_and_one_arcane() {
        assert_eq!(artifact_mods().len(), 20);
        assert_eq!(artifact_arcanes().len(), 5);
        for s in focus_schools() {
            assert!(s.artifact.is_some(), "{} has no artifact", s.id);
        }
        for m in artifact_mods() {
            assert!(focus_school(&m.school).is_some(), "{}: school {}", m.id, m.school);
            if let Some(b) = &m.bonus {
                assert!(b.per == "unique_school" || focus_school(&b.per).is_some(), "{}: bonus per {}", m.id, b.per);
                assert_eq!(m.description.lines().count(), 2, "{}: the bonus is the second line", m.id);
            }
        }

        let mut b = build(&[]);
        let pick = |mods: &[&str], arcane: Option<&str>| OperatorPick {
            school: "madurai".into(),
            assumed: vec![],
            artifact: ArtifactPick {
                mods: mods.iter().map(|s| s.to_string()).collect(),
                arcane: arcane.map(str::to_string),
            },
        };
        b.operator = Some(pick(&["ubri_kaneph", "da_ren", "omn_evi", "yar_dal", "sey_taph"], Some("zid_an_asheir")));
        assert!(resolve(&b).unwrap().refused.is_empty());
        b.operator = Some(pick(&["ubri_kaneph", "ubri_kaneph"], None));
        assert!(resolve(&b).unwrap().refused.iter().any(|r| r.contains("seated twice")));
        b.operator = Some(pick(&["ubri_kaneph", "da_ren", "omn_evi", "yar_dal", "sey_taph", "evir_ti"], None));
        assert!(resolve(&b).unwrap().refused.iter().any(|r| r.contains("5 mod slots")));
        b.operator = Some(pick(&[], Some("arcane_grace")));
        assert!(resolve(&b).unwrap().refused.iter().any(|r| r.contains("unknown artifact arcane")));

        // M93: two Madurai, two Vazarin, one Naramon.
        let seated: Vec<&ArtifactMod> = ["ubri_kaneph", "sil_tabol", "da_ren", "metem_hakh", "omn_evi"]
            .iter()
            .map(|id| artifact_mod_by_id(id).unwrap())
            .collect();
        let card = |id: &str| artifact_mod_by_id(id).unwrap().card_with(&seated);
        assert_eq!(card("ubri_kaneph"), ["+60% Damage to Amps", "+20% Amp Damage"], "Vazarin and Naramon, not Madurai");
        assert_eq!(card("metem_hakh")[1], "+30% Operator Health & Shields", "Madurai and Naramon, not Vazarin");
        assert_eq!(card("sil_tabol")[1], "+30% Status Damage", "each Vazarin card");
        assert_eq!(card("da_ren")[1], "+0 Operator Shields", "no Unairu card");
    }

    /// W`Shield`'s formula at Valkyr's 185, Catalyzing Shields' x0.20 and 1.33 s,
    /// and a cast that refills past max giving the full gate under every reading.
    #[test]
    fn the_shield_gate_follows_the_shield_page_and_catalyzing_shields() {
        let bare = resolve(&build(&[])).unwrap();
        assert!(close(bare.shield_gate.full_seconds, (185.0f64 / 350.0).powf(0.65) + 1.0 / 3.0));
        assert!(bare.shield_gate.casts.is_empty(), "no refill source, no re-opened gate");

        let mut b = build(&["catalyzing_shields", "augur_secrets", "augur_message"]);
        b.aura = Some(SlotPick { id: "brief_respite".into(), rank: None });
        let r = resolve(&b).unwrap();
        assert!(close(r.stat(FrameStat::Shield).value, 185.0 * 0.2));
        assert!(close(r.shield_gate.full_seconds, 1.33));
        // two Augur cards (80%) + Brief Respite (150%)
        assert!(close(r.shield_gate.energy_to_shield, 2.3));
        let warcry = r.shield_gate.casts.iter().find(|c| c.ability == "warcry").unwrap();
        assert!(warcry.full, "75 energy x2.3 refills 37 shields");
        let t = r.tags.iter().find(|t| t.from == "shield_gate").expect("the derived tag");
        assert_eq!(t.tag, Capability::Invulnerable);

        // ONE Augur card (40%): Rip Line's 25 energy restores 10 of 37 shields,
        // and Catalyzing Shields still gives its 1.33 s (MEASUREMENTS M92).
        let partial = resolve(&build(&["catalyzing_shields", "augur_secrets"])).unwrap();
        let rip = partial.shield_gate.casts.iter().find(|c| c.ability == "rip_line").unwrap();
        assert!(!rip.full);
        assert!(close(rip.seconds, 1.33));
        // …and without it, the gate is the refill's own: W`Shield`'s formula at 10.
        let bare = resolve(&build(&["augur_secrets"])).unwrap();
        let rip = bare.shield_gate.casts.iter().find(|c| c.ability == "rip_line").unwrap();
        assert!(close(rip.seconds, 10.0 / 180.0 + 1.0 / 3.0));
    }

    #[test]
    fn aura_capacity_doubles_matched_and_floors_mismatched() {
        assert_eq!(aura_capacity(7, "madurai", Some("madurai")), 14);
        assert_eq!(aura_capacity(7, "madurai", None), 7);
        assert_eq!(aura_capacity(9, "madurai", Some("naramon")), 7);
        assert_eq!(aura_capacity(5, "madurai", Some("naramon")), 4);
    }
}
