use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct CompanionBuild {
    pub companion: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub assembly: CompanionAssembly,
    #[serde(default = "gilded_default", skip_serializing_if = "is_gilded")]
    pub gilded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gild_polarity: Option<String>,
    #[serde(default)]
    pub mods: Vec<crate::data::warframes::SlotPick>,
}
fn gilded_default() -> bool { true }
fn is_gilded(value: &bool) -> bool { *value }

impl Default for CompanionBuild {
    fn default() -> Self {
        Self { companion: String::new(), assembly: CompanionAssembly::new(), gilded: true, gild_polarity: None, mods: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FloorSource {
    pub stat: String,
    pub companion: String,
    pub assembly: CompanionAssembly,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedCompanion {
    pub companion: String,
    pub name: String,
    pub base: BodyStats,
    pub stats: BodyStats,
    pub assembly: CompanionAssembly,
    pub gild_polarity: Option<String>,
    pub polarities: Vec<String>,
    pub floor_sources: Vec<FloorSource>,
    pub unmodelled: Vec<String>,
    pub refused: Vec<String>,
    #[serde(skip)]
    pub element_per_hit: Option<crate::model::ElementPerHit>,
}

/// Gilding grants one ordinary polarity; Umbra and universal require their own Forma.
pub const GILD_POLARITIES: [&str; 6] = ["madurai", "naramon", "vazarin", "zenurik", "unairu", "penjaga"];

impl CompanionBuild {
    pub fn element_per_hit(&self) -> Option<crate::model::ElementPerHit> {
        resolve(self).ok().and_then(|r| r.element_per_hit)
    }
}

fn mod_bonus(build: &CompanionBuild, host: &str, refused: &mut Vec<String>) -> (BodyStats, Option<crate::model::ElementPerHit>) {
    let mut bonus = BodyStats::default();
    let mut pile = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut families = std::collections::BTreeSet::new();
    let mut precepts = std::collections::BTreeMap::new();
    let limits = companion(host).map(host_precept_limits).unwrap_or_default();
    for (index, p) in build.mods.iter().enumerate() {
        let Some(m) = seatable_by(host).find(|m| m.id == p.id) else { refused.push(format!("cannot seat {} on {host}", p.id)); continue };
        if index >= COMPANION_MOD_SLOTS { refused.push("more than ten mods".into()); continue; }
        if !seen.insert(m.id.as_str()) { refused.push(format!("duplicate mod: {}", m.id)); continue; }
        if let Some(family) = &m.family {
            if !families.insert(family) { refused.push(format!("incompatible mod family: {family}")); continue; }
        }
        if m.precept {
            let count = precepts.entry(&m.compat).or_insert(0usize);
            *count += 1;
            if limits.get(&m.compat).is_some_and(|limit| *count > *limit) {
                refused.push(format!("too many {} precepts", m.compat)); continue;
            }
        }
        let rank = p.rank.unwrap_or(m.max_rank);
        if rank > m.max_rank { refused.push(format!("invalid rank for {}", m.id)); continue; }
        for (stat, value) in &m.stat_bonuses {
            let x = crate::data::warframes::at_rank(*value, rank, m.max_rank);
            match stat.as_str() { "health" => bonus.health += x, "shield" => bonus.shield += x, "armor" => bonus.armor += x, _ => {} }
        }
        pile = pile.or_else(|| m.element_per_hit_at(rank));
    }
    (bonus, pile)
}

pub fn resolve(build: &CompanionBuild) -> Result<ResolvedCompanion, String> {
    let host = companion(&build.companion).ok_or_else(|| format!("unknown companion: {}", build.companion))?;
    let mut refused = Vec::new();
    let (mods, pile) = mod_bonus(build, &host.id, &mut refused);
    if let Some(scope) = host.prototype_scope.as_deref().and_then(carrier_scope) {
        if !build.assembly.is_empty() || build.gild_polarity.is_some() { return Err("a prototype has no selectable assembly or gilding polarity".into()); }
        let mut sources = Vec::new();
        let mut minimum = BodyStats { health: f64::INFINITY, shield: f64::INFINITY, armor: f64::INFINITY };
        let mut minimum_base = minimum;
        let mut pols: Option<Vec<String>> = None;
        for id in &scope.companions {
            let member = companion(id).ok_or_else(|| format!("unknown carrier: {id}"))?;
            for a in assemblies(member) {
                let mut body = assemble(member, &a, build.gilded)?;
                let bare = body.base.apply(body.bonus);
                body.bonus.add(mods);
                let stats = body.base.apply(body.bonus);
                for stat in BODY_STATS {
                    let (min, base) = match stat { "health" => (&mut minimum.health, &mut minimum_base.health),
                        "shield" => (&mut minimum.shield, &mut minimum_base.shield), _ => (&mut minimum.armor, &mut minimum_base.armor) };
                    *base = base.min(bare.get(stat));
                    if stats.get(stat) < *min {
                        *min = stats.get(stat);
                        sources.retain(|s: &FloorSource| s.stat != stat);
                        sources.push(FloorSource { stat: stat.into(), companion: id.clone(), assembly: a.clone(), value: *min });
                    }
                }
                if let Some(pool) = &mut pols {
                    let mut available = body.polarities;
                    pool.retain(|p| available.iter().position(|q| q == p).map(|i| { available.remove(i); }).is_some());
                } else { pols = Some(body.polarities); }
            }
        }
        if !minimum.health.is_finite() { return Err("empty prototype carrier scope".into()); }
        return Ok(ResolvedCompanion { companion: host.id.clone(), name: host.name.clone(), base: minimum_base, stats: minimum,
            assembly: CompanionAssembly::new(), gild_polarity: None, polarities: pols.unwrap_or_default(), floor_sources: sources,
            unmodelled: vec!["Companion movement, survival events, precept AI and owner links are not simulated.".into()], refused, element_per_hit: pile });
    }
    let mut body = assemble(host, &build.assembly, build.gilded)?;
    let gild_polarity = if host.assembly.is_some() && build.gilded {
        let p = build.gild_polarity.as_deref().unwrap_or(GILD_POLARITIES[0]);
        if !GILD_POLARITIES.contains(&p) { return Err("invalid gilding polarity".into()); }
        body.polarities.push(p.into());
        Some(p.to_string())
    } else { None };
    let base = body.base.apply(body.bonus);
    body.bonus.add(mods);
    body.unmodelled.push("Companion movement, survival events, precept AI and owner links are not simulated.".into());
    Ok(ResolvedCompanion { companion: host.id.clone(), name: host.name.clone(), base, stats: body.base.apply(body.bonus),
        assembly: body.assembly, gild_polarity, polarities: body.polarities, floor_sources: Vec::new(), unmodelled: body.unmodelled,
        refused, element_per_hit: pile })
}

/// Invalid hosts never contribute their mods to a weapon's default carrier.
pub fn weapon_build(weapon: &str, asked: Option<CompanionBuild>) -> Option<CompanionBuild> {
    let scope = weapon_scope(weapon)?;
    Some(asked.filter(|b| companion(&b.companion).is_some_and(|h| can_carry(scope, h)))
        .unwrap_or_else(|| CompanionBuild { companion: scope.prototype.clone(), ..Default::default() }))
}

/// Compatibility floor for ranged robotic weapons, derived from legal bodies.
pub fn sentinel_wielder() -> &'static crate::data::tenno::Tenno {
    static T: std::sync::OnceLock<crate::data::tenno::Tenno> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let r = crate::data::companions::resolve(&crate::data::companions::CompanionBuild {
            companion: crate::data::companions::PROTOTYPE_COMPANION.into(), ..Default::default()
        }).expect("default companion scope");
        let mut t = crate::data::tenno::default_tenno().clone();
        t.id = r.companion;
        t.name = r.name;
        t.health = r.stats.health;
        t.shield = r.stats.shield;
        t.armor = r.stats.armor;
        t.energy = 0.0;
        t.sprint = 0.0;
        t
    })
}
