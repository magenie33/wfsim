use super::*;

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
    pub(super) fn from_kind(kind: &str) -> Option<FrameStat> {
        let id = kind.strip_suffix("_bonus")?;
        FrameStat::ALL.into_iter().find(|s| s.id() == id)
    }

    pub(super) fn parse(id: &str) -> FrameStat {
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

    pub(super) fn parse(path: &str, id: &str) -> Capability {
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
pub(super) struct RawTag {
    pub(super) tag: String,
    pub(super) when: String,
    /// The infused version's timing, where its page states a different one.
    #[serde(default)]
    pub(super) infused_when: Option<String>,
    /// The infused version grants no such tag (Omamori).
    #[serde(default)]
    pub(super) not_when_infused: bool,
}

pub(super) fn tags_of(path: &str, raw: &[RawTag]) -> Vec<TagGrant> {
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
