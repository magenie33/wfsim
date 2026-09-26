use super::*;

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
    /// An ARCANE's rule that asks more than a stat bucket can answer; refused
    /// on any other card at load.
    Arcane(ArcaneRule),
}

/// **WHAT A WARFRAME ARCANE DOES TO ABILITY STRENGTH**, each read where the
/// fact it asks for is known. Every ladder is per rank, fractions of 1.
#[derive(Debug, Clone, PartialEq)]
pub enum ArcaneRule {
    /// Arcane Bellicose: `round(Max Health ÷ per × increase)`% and not whole
    /// steps of `per` (W`Arcane_Bellicose`), up to `cap`.
    StrengthPerMaxHealth { per: f64, increase: Vec<f64>, cap: Vec<f64> },
    /// Molt Augmented: a stack per kill that "persists until death"
    /// (W`Molt_Augmented`). The build states the stacks it opens with.
    StrengthPerKill { per_stack: Vec<f64>, max_stacks: u32 },
    /// Molt Vigor: the next Warframe cast after an Operator ability.
    StrengthAfterOperatorAbility(Vec<f64>),
    /// Arcane Power Ramp: each cast gives the next one a stack; "The bonus
    /// resets after casting the same ability consecutively".
    StrengthPerCastStack { per_stack: Vec<f64>, max_stacks: u32 },
    /// Arcane Fury, Arcane Strike: a buff armed by the WEAPON on one slot, run
    /// by the same stacking machinery a mod's is (`model::StackingBuff`).
    WeaponBuff {
        slot: String,
        trigger: crate::model::BuffTrigger,
        grant: crate::model::BuffGrant,
        chance: f64,
        duration: f64,
        per_stack: Vec<f64>,
    },
}

/// A buff the wielder's arcane arms on a weapon of `slot`, at its rank.
#[derive(Debug, Clone, PartialEq)]
pub struct WielderBuff {
    pub slot: String,
    pub buff: crate::model::StackingBuff,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SourceFile {
    #[serde(default)]
    pub(super) url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawEffect {
    pub(super) kind: String,
    #[serde(rename = "rankMax", default)]
    pub(super) rank_max: f64,
    #[serde(default)]
    pub(super) value: f64,
    /// A per-rank ladder, where a card's values do not follow `at_rank`.
    #[serde(default)]
    pub(super) ranks: Vec<f64>,
    #[serde(default)]
    pub(super) text: Option<String>,
    #[serde(default)]
    pub(super) applies_to: Option<String>,
    /// The unit a `*_per_max_health` rule counts in (250 health).
    #[serde(default)]
    pub(super) per: f64,
    /// A per-rank ceiling beside `ranks`.
    #[serde(default)]
    pub(super) cap: Vec<f64>,
    #[serde(default)]
    pub(super) max_stacks: u32,
    #[serde(default)]
    pub(super) slot: Option<String>,
    #[serde(default)]
    pub(super) trigger: Option<String>,
    #[serde(default)]
    pub(super) grants: Option<String>,
    #[serde(default)]
    pub(super) chance: Option<f64>,
    #[serde(default)]
    pub(super) duration: Option<f64>,
}

pub(super) fn effect(path: &str, e: &RawEffect) -> FrameEffect {
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
        "ability_strength_per_max_health" | "ability_strength_per_kill" | "ability_strength_after_operator_ability"
        | "ability_strength_per_cast_stack" => {
            assert!(!e.ranks.is_empty(), "{path}: `{}` carries its `ranks`", e.kind);
            FrameEffect::Arcane(match e.kind.as_str() {
                "ability_strength_per_max_health" => {
                    assert!(e.per > 0.0 && e.cap.len() == e.ranks.len(), "{path}: `per` and a `cap` per rank");
                    ArcaneRule::StrengthPerMaxHealth { per: e.per, increase: e.ranks.clone(), cap: e.cap.clone() }
                }
                "ability_strength_per_kill" | "ability_strength_per_cast_stack" => {
                    assert!(e.max_stacks > 0, "{path}: `{}` carries its `max_stacks`", e.kind);
                    if e.kind == "ability_strength_per_kill" {
                        ArcaneRule::StrengthPerKill { per_stack: e.ranks.clone(), max_stacks: e.max_stacks }
                    } else {
                        ArcaneRule::StrengthPerCastStack { per_stack: e.ranks.clone(), max_stacks: e.max_stacks }
                    }
                }
                _ => ArcaneRule::StrengthAfterOperatorAbility(e.ranks.clone()),
            })
        }
        "weapon_buff" => {
            let word = |w: &Option<String>, what: &str| {
                w.clone().unwrap_or_else(|| panic!("{path}: a `weapon_buff` names its `{what}`"))
            };
            assert!(!e.ranks.is_empty(), "{path}: `weapon_buff` carries its `ranks`");
            let trigger = word(&e.trigger, "trigger");
            let grants = word(&e.grants, "grants");
            FrameEffect::Arcane(ArcaneRule::WeaponBuff {
                slot: word(&e.slot, "slot"),
                trigger: crate::model::BuffTrigger::from_id(&trigger)
                    .unwrap_or_else(|| panic!("{path}: unknown trigger `{trigger}`")),
                grant: crate::model::BuffGrant::from_id(&grants)
                    .unwrap_or_else(|| panic!("{path}: unknown grant `{grants}`")),
                chance: e.chance.unwrap_or(1.0),
                duration: e.duration.unwrap_or_else(|| panic!("{path}: a `weapon_buff` states its `duration`")),
                per_stack: e.ranks.clone(),
            })
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

pub(super) fn leak_all(prefix: &str) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
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
pub(super) struct RawMod {
    #[serde(default)]
    pub(super) tags: Vec<RawTag>,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) rarity: String,
    pub(super) polarity: String,
    pub(super) base_drain: u32,
    pub(super) max_rank: u32,
    #[serde(default)]
    pub(super) exilus: bool,
    #[serde(default)]
    pub(super) family: Option<String>,
    #[serde(default)]
    pub(super) set: Option<String>,
    #[serde(default)]
    pub(super) set_bonus: BTreeMap<u32, f64>,
    #[serde(default)]
    pub(super) augments: Option<String>,
    #[serde(default)]
    pub(super) internal_name: Option<String>,
    #[serde(default)]
    pub(super) description: String,
    pub(super) effects: Vec<RawEffect>,
    #[serde(default)]
    pub(super) source: SourceFile,
}

pub(super) fn aura_card(a: &crate::data::auras::AuraDef) -> WarframeMod {
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
                assert!(
                    !r.effects.iter().any(|e| matches!(effect(p, e), FrameEffect::Arcane(_))),
                    "{p}: an arcane's rule on a card that is not an arcane"
                );
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
        out.extend(crate::data::auras::all().iter().map(aura_card));
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
pub(super) fn card_lines(effects: &[FrameEffect], description: &str, rank: u32, max_rank: u32) -> Vec<String> {
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
pub(super) struct RawArcane {
    #[serde(default)]
    pub(super) tags: Vec<RawTag>,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) rarity: String,
    pub(super) max_rank: u32,
    #[serde(default)]
    pub(super) internal_name: Option<String>,
    #[serde(default)]
    pub(super) description: String,
    pub(super) effects: Vec<RawEffect>,
    #[serde(default)]
    pub(super) source: SourceFile,
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
