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
    pub passive: String,
    /// What the passive grants, as tags.
    pub passive_tags: Vec<TagGrant>,
    /// A passive that is a meter on melee damage, simulated (`crate::data::rage`).
    pub rage: Option<RageSpec>,
    /// Ability ids in slot order.
    pub abilities: Vec<String>,
    pub url: Option<String>,
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
    pub(super) passive: String,
    #[serde(default)]
    pub(super) passive_tags: Vec<RawTag>,
    #[serde(default)]
    pub(super) rage: Option<RageSpec>,
    pub(super) abilities: Vec<String>,
    #[serde(default)]
    pub(super) source: SourceFile,
}

/// The floor frame every weapon is held by when nothing else is linked.
pub const PROTOTYPE: &str = "prototype";

pub fn warframes() -> &'static [WarframeDef] {
    static F: OnceLock<Vec<WarframeDef>> = OnceLock::new();
    F.get_or_init(|| {
        leak_all("warframes/")
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
