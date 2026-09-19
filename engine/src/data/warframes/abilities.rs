use super::*;

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
pub(super) struct RawChannel {
    pub(super) ability: String,
    pub(super) value: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawStat {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) value: f64,
    pub(super) unit: String,
    pub(super) scales_with: String,
    #[serde(default)]
    pub(super) helminth_value: Option<f64>,
    #[serde(default)]
    pub(super) min: Option<f64>,
    #[serde(default)]
    pub(super) cap: Option<f64>,
    #[serde(default)]
    pub(super) adds_to_base: Option<String>,
    #[serde(default)]
    pub(super) channel_multiplier: Option<RawChannel>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawAbility {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) frame: String,
    #[serde(default)]
    pub(super) slot: Option<u8>,
    pub(super) energy_cost: f64,
    #[serde(default)]
    pub(super) cost_type: Option<String>,
    #[serde(default)]
    pub(super) subsumable: bool,
    #[serde(default)]
    pub(super) augments: Vec<String>,
    #[serde(default)]
    pub(super) icon: String,
    #[serde(default)]
    pub(super) description: String,
    #[serde(default)]
    pub(super) drain_per_second: Option<f64>,
    #[serde(default)]
    pub(super) stats: Vec<RawStat>,
    #[serde(default)]
    pub(super) unmodelled: Vec<String>,
    #[serde(default)]
    pub(super) tags: Vec<RawTag>,
    #[serde(default)]
    pub(super) infused_energy_cost: Option<f64>,
    #[serde(default)]
    pub(super) infused_notes: Vec<String>,
    #[serde(default)]
    pub(super) internal_name: Option<String>,
    #[serde(default)]
    pub(super) source: SourceFile,
}

pub(super) fn scaling(path: &str, s: &str) -> Scaling {
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
