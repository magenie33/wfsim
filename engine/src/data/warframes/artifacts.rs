use super::*;

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

pub(super) fn source_url<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(SourceFile::deserialize(d)?.url)
}

pub(super) fn load_sorted<T: serde::de::DeserializeOwned>(prefix: &str, id: fn(&T) -> &str) -> Vec<T> {
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
