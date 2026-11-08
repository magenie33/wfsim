use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{BodyStats, Companion};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CompanionPart {
    pub id: String,
    pub name: String,
    pub family: String,
    pub slot: String,
    pub internal_name: String,
    #[serde(default)]
    pub modify_base: bool,
    #[serde(default)]
    pub base: Option<BodyStats>,
    #[serde(default)]
    pub bonus: BodyStats,
    #[serde(default)]
    pub ungilded_bonus: BodyStats,
    #[serde(default)]
    pub polarity: Option<String>,
    #[serde(default)]
    pub unmodelled: Vec<String>,
}

pub type CompanionAssembly = BTreeMap<String, String>;

#[derive(Deserialize)]
struct PartCatalog { parts: Vec<CompanionPart>, slot_labels: BTreeMap<String, String> }

fn part_catalog() -> &'static PartCatalog {
    static P: OnceLock<PartCatalog> = OnceLock::new();
    P.get_or_init(|| {
        let text = crate::data::file("companion_parts.yaml").expect("companion parts");
        let mut catalog = serde_norway::from_str::<PartCatalog>(text).expect("parse companion parts");
        catalog.parts.sort_by(|a, b| a.id.cmp(&b.id));
        catalog
    })
}

pub fn companion_parts() -> &'static [CompanionPart] { &part_catalog().parts }
pub fn companion_part_labels() -> &'static BTreeMap<String, String> { &part_catalog().slot_labels }

pub fn parts_for(host: &Companion) -> Vec<&'static CompanionPart> {
    host.assembly.as_ref().map_or_else(Vec::new, |family| companion_parts().iter().filter(|p| &p.family == family).collect())
}

pub fn default_parts(host: &Companion) -> CompanionAssembly {
    let mut parts = CompanionAssembly::new();
    for part in parts_for(host) { parts.entry(part.slot.clone()).or_insert_with(|| part.id.clone()); }
    parts
}

/// Missing slots use complete legal defaults; foreign parts and extra slots are errors.
pub fn normalize_parts(host: &Companion, asked: &CompanionAssembly) -> Result<CompanionAssembly, String> {
    let mut complete = default_parts(host);
    for (slot, id) in asked {
        if !parts_for(host).iter().any(|p| &p.slot == slot && &p.id == id) {
            return Err(format!("{id} is not a {slot} for {}", host.id));
        }
        complete.insert(slot.clone(), id.clone());
    }
    Ok(complete)
}

pub fn assemblies(host: &Companion) -> Vec<CompanionAssembly> {
    let mut slots: BTreeMap<String, Vec<&CompanionPart>> = BTreeMap::new();
    for part in parts_for(host) { slots.entry(part.slot.clone()).or_default().push(part); }
    let mut out = vec![CompanionAssembly::new()];
    for (slot, parts) in slots {
        out = out.into_iter().flat_map(|assembly| parts.iter().map(|p| {
            let mut a = assembly.clone(); a.insert(slot.clone(), p.id.clone()); a
        }).collect::<Vec<_>>()).collect();
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct AssembledBody {
    pub assembly: CompanionAssembly,
    pub base: BodyStats,
    pub bonus: BodyStats,
    pub polarities: Vec<String>,
    pub unmodelled: Vec<String>,
}

/// Client Max*Modifier values are gilded and add to body mods; Hound cores replace base.
pub fn assemble(host: &Companion, asked: &CompanionAssembly, gilded: bool) -> Result<AssembledBody, String> {
    let assembly = normalize_parts(host, asked)?;
    if host.prototype_scope.is_some() { return Err("a prototype has no concrete assembly".into()); }
    let mut body = AssembledBody { assembly, base: host.stats.unwrap_or_default(),
        bonus: BodyStats::default(), polarities: host.polarities.clone(), unmodelled: Vec::new() };
    let mut base_bonus = BodyStats::default();
    for part in parts_for(host).into_iter().filter(|p| body.assembly.get(&p.slot) == Some(&p.id)) {
        if let Some(base) = part.base { body.base = base; }
        let bonus = if gilded { part.bonus } else { part.ungilded_bonus };
        if part.modify_base { base_bonus.add(bonus); } else { body.bonus.add(bonus); }
        if let Some(p) = &part.polarity { body.polarities.push(p.clone()); }
        body.unmodelled.extend(part.unmodelled.iter().cloned());
    }
    body.base = body.base.apply(base_bonus);
    body.unmodelled.sort(); body.unmodelled.dedup();
    Ok(body)
}
