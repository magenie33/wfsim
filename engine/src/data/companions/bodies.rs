use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Unmodded rank-30 body attributes. Modular bonuses remain separate from base.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct BodyStats {
    pub health: f64,
    pub shield: f64,
    pub armor: f64,
}

impl BodyStats {
    pub fn get(self, stat: &str) -> f64 {
        match stat { "health" => self.health, "shield" => self.shield, "armor" => self.armor, _ => 0.0 }
    }

    pub fn apply(self, bonus: Self) -> Self {
        Self { health: self.health * (1.0 + bonus.health), shield: self.shield * (1.0 + bonus.shield), armor: self.armor * (1.0 + bonus.armor) }
    }

    pub fn add(&mut self, other: Self) {
        self.health += other.health;
        self.shield += other.shield;
        self.armor += other.armor;
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Companion {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub stats: Option<BodyStats>,
    #[serde(default)]
    pub polarities: Vec<String>,
    #[serde(default)]
    pub assembly: Option<String>,
    #[serde(default)]
    pub prototype_scope: Option<String>,
    #[serde(default)]
    pub precept_limits: std::collections::BTreeMap<String, usize>,
    #[serde(default)]
    pub internal_name: Option<String>,
    #[serde(default)]
    pub source: Option<serde_json::Value>,
}

/// A carrier set and its default. Weapon membership is declared only here.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CarrierScope {
    pub id: String,
    pub companions: Vec<String>,
    pub weapons: Vec<String>,
    pub default: String,
    #[serde(default)]
    pub unmodelled_weapons: Vec<CarrierWeapon>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CarrierWeapon {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub modelled: bool,
}

pub const PROTOTYPE_COMPANION: &str = "prototype_companion";
pub const BODY_STATS: [&str; 3] = ["health", "shield", "armor"];

pub fn companions() -> &'static [Companion] {
    static C: OnceLock<Vec<Companion>> = OnceLock::new();
    C.get_or_init(|| {
        let mut all: Vec<Companion> = crate::data::files_under("companions/")
            .map(|(p, text)| serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}")))
            .collect();
        let mut ids = std::collections::BTreeSet::new();
        for c in &all {
            assert!(ids.insert(&c.id), "duplicate companion: {}", c.id);
            if let Some(s) = c.stats { assert!(s.health > 0.0 && s.shield >= 0.0 && s.armor >= 0.0, "{}: invalid body stats", c.id); }
        }
        all.sort_by(|a, b| a.prototype_scope.is_some().cmp(&b.prototype_scope.is_some()).then_with(|| a.name.cmp(&b.name)));
        all
    })
}

pub fn companion(id: &str) -> Option<&'static Companion> {
    companions().iter().find(|c| c.id == id)
}

pub fn carrier_scopes() -> &'static [CarrierScope] {
    static S: OnceLock<Vec<CarrierScope>> = OnceLock::new();
    S.get_or_init(|| {
        #[derive(Deserialize)]
        struct File { scopes: Vec<CarrierScope> }
        let text = crate::data::file("companion_scopes.yaml").expect("companion carrier scopes");
        serde_norway::from_str::<File>(text).expect("parse companion carrier scopes").scopes
    })
}

pub fn carrier_scope(id: &str) -> Option<&'static CarrierScope> {
    carrier_scopes().iter().find(|s| s.id == id)
}

pub fn weapon_scope(weapon: &str) -> Option<&'static CarrierScope> {
    carrier_scopes().iter().find(|s| s.weapons.iter().any(|w| w == weapon))
}

/// A prototype can hold a weapon only if its entire represented set can.
pub fn can_carry(scope: &CarrierScope, host: &Companion) -> bool {
    if let Some(id) = &host.prototype_scope {
        carrier_scope(id).is_some_and(|s| !s.companions.is_empty() && s.companions.iter().all(|h| scope.companions.contains(h)))
    } else {
        scope.companions.contains(&host.id)
    }
}

pub fn carriers(weapon: &str) -> Vec<&'static Companion> {
    weapon_scope(weapon).map_or_else(Vec::new, |scope| companions().iter().filter(|c| can_carry(scope, c)).collect())
}

/// Loaded weapon names win; scope metadata names only weapons with no combat entry.
pub fn carrier_weapons(host: &Companion) -> Vec<CarrierWeapon> {
    carrier_scopes().iter().filter(|s| can_carry(s, host)).flat_map(|s| s.weapons.iter().filter_map(|id| {
        crate::data::weapons::spec(id).map(|w| CarrierWeapon { id: id.clone(), name: w.name.clone(), modelled: true })
            .or_else(|| s.unmodelled_weapons.iter().find(|w| &w.id == id).cloned())
    })).collect()
}

/// The mod pool of a prototype is the intersection, never the union, of its bodies.
pub fn host_groups(host: &Companion) -> Vec<String> {
    let Some(scope) = host.prototype_scope.as_deref().and_then(carrier_scope) else { return host.groups.clone() };
    let mut members = scope.companions.iter().filter_map(|id| companion(id));
    let Some(first) = members.next() else { return Vec::new() };
    let mut groups = first.groups.clone();
    for c in members { groups.retain(|g| c.groups.contains(g)); }
    groups
}

pub fn host_precept_limits(host: &Companion) -> std::collections::BTreeMap<String, usize> {
    let Some(scope) = host.prototype_scope.as_deref().and_then(carrier_scope) else { return host.precept_limits.clone() };
    let mut out = std::collections::BTreeMap::new();
    for c in scope.companions.iter().filter_map(|id| companion(id)) {
        for (group, limit) in &c.precept_limits {
            out.entry(group.clone()).and_modify(|n: &mut usize| *n = (*n).min(*limit)).or_insert(*limit);
        }
    }
    out
}
