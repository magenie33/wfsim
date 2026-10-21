use super::*;

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
    pub passive: crate::prose::Prose,
    /// What the passive grants, as tags.
    pub passive_tags: Vec<TagGrant>,
    /// A passive that is a meter on melee damage, simulated (`crate::data::rage`).
    pub rage: Option<RageSpec>,
    /// A passive the engine does not run, and whether a weapon's fight would read it.
    pub passive_unmodelled: Option<PassiveReach>,
    /// The one ability a damage-buff infusion may replace on this frame (W`Helminth`
    /// §Damage buff restrictions); `None` leaves every slot open.
    pub helminth_damage_buff_slot: Option<String>,
    /// Ability ids in slot order.
    pub abilities: Vec<String>,
    pub url: Option<String>,
}

/// WHERE AN UNMODELLED PASSIVE WOULD SHOW. `Weapon` moves a number a weapon's fight
/// reads, so the weapon page admits it too; `Frame` is the frame's own side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassiveReach {
    Weapon,
    Frame,
}

impl PassiveReach {
    pub fn id(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Frame => "frame",
        }
    }
}

/// RAGE's numbers, as the frame's file states them — `crate::data::rage` runs them.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RageSpec {
    /// Shares of base damage: per melee hit landed, per melee kill, and the cap.
    pub per_hit: f64,
    pub per_kill: f64,
    pub cap: f64,
    /// Seconds without building before the decay starts.
    pub idle_seconds: f64,
    /// λ of the decay curve, and the slope of its linear tail in meter per second.
    pub decay_rate: f64,
    pub tail_per_second: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawFrame {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) internal_name: Option<String>,
    /// Absent only on the Prototype, whose stats are `data/tenno/default.yaml`'s.
    #[serde(default)]
    pub(super) health: Option<f64>,
    #[serde(default)]
    pub(super) shield: Option<f64>,
    #[serde(default)]
    pub(super) polarities: Vec<String>,
    #[serde(default)]
    pub(super) aura_polarity: Option<String>,
    #[serde(default)]
    pub(super) exilus_polarity: Option<String>,
    #[serde(default)]
    pub(super) passive: crate::prose::Prose,
    #[serde(default)]
    pub(super) passive_tags: Vec<RawTag>,
    #[serde(default)]
    pub(super) rage: Option<RageSpec>,
    #[serde(default)]
    pub(super) passive_unmodelled: Option<PassiveReach>,
    #[serde(default)]
    pub(super) helminth_damage_buff_slot: Option<String>,
    pub(super) abilities: Vec<String>,
    #[serde(default)]
    pub(super) source: SourceFile,
}

/// The floor frame every weapon is held by when nothing else is linked.
pub const PROTOTYPE: &str = "prototype";

/// Every Warframe, in file order — THE PROTOTYPE LAST: it is the floor a weapon
/// falls back to, never a frame anyone picks first, so every list that reads
/// this one (the home grid, the pickers, the catalogue) ends with it.
pub fn warframes() -> &'static [WarframeDef] {
    static F: OnceLock<Vec<WarframeDef>> = OnceLock::new();
    F.get_or_init(|| {
        let mut all: Vec<WarframeDef> = leak_all("warframes/")
            .map(|(p, text)| {
                let r: RawFrame = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                // THE PROTOTYPE'S FIVE NUMBERS ARE THE FIGHT'S FLOOR, read from the
                // one file that states them; every other frame is the roster plus
                // its own rank-30 health and shield.
                let (health, shield, armor, energy, sprint) = if r.id == PROTOTYPE {
                    assert!(r.health.is_none() && r.shield.is_none(), "{p}: the Prototype's stats are data/tenno/default.yaml's");
                    let t = crate::data::tenno::default_tenno();
                    (t.health, t.shield, t.armor, t.energy, t.sprint)
                } else {
                    let roster = crate::data::tenno::frame(&r.id)
                        .unwrap_or_else(|| panic!("{p}: `{}` is not in data/frames.yaml", r.id));
                    let own = |v: Option<f64>, what: &str| v.unwrap_or_else(|| panic!("{p}: no {what}"));
                    (own(r.health, "health"), own(r.shield, "shield"), roster.armor, roster.energy, roster.sprint)
                };
                if let Some(only) = &r.helminth_damage_buff_slot {
                    assert!(r.abilities.contains(only), "{p}: helminth_damage_buff_slot `{only}` is not one of its abilities");
                }
                WarframeDef {
                    id: r.id,
                    name: r.name,
                    internal_name: r.internal_name,
                    health,
                    shield,
                    armor,
                    energy,
                    sprint,
                    polarities: r.polarities,
                    aura_polarity: r.aura_polarity,
                    exilus_polarity: r.exilus_polarity,
                    passive_tags: tags_of(p, &r.passive_tags),
                    passive: r.passive,
                    rage: r.rage,
                    passive_unmodelled: r.passive_unmodelled,
                    helminth_damage_buff_slot: r.helminth_damage_buff_slot,
                    abilities: r.abilities,
                    url: r.source.url,
                }
            })
            .collect();
        all.sort_by_key(|f| f.id == PROTOTYPE);
        all
    })
}

pub fn warframe(id: &str) -> Option<&'static WarframeDef> {
    warframes().iter().find(|f| f.id == id)
}

// ---- a build --------------------------------------------------------------
