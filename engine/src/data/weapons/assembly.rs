//! Modular parts share one build axis; their composition belongs to the weapon family.
use super::{kitguns, zaws, WeaponSpec};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assembly {
    Kitgun(kitguns::Assembly),
    Zaw(zaws::Assembly),
}

impl From<kitguns::Assembly> for Assembly {
    fn from(value: kitguns::Assembly) -> Self { Self::Kitgun(value) }
}
impl From<zaws::Assembly> for Assembly {
    fn from(value: zaws::Assembly) -> Self { Self::Zaw(value) }
}

impl Assembly {
    pub fn grip(&self) -> &str {
        match self { Self::Kitgun(a) => &a.grip, Self::Zaw(a) => &a.grip }
    }
    pub fn component(&self) -> &str {
        match self { Self::Kitgun(a) => &a.loader, Self::Zaw(a) => &a.link }
    }
    pub fn component_key(&self) -> &'static str {
        match self { Self::Kitgun(_) => "loader", Self::Zaw(_) => "link" }
    }
    pub fn parts(&self) -> Value {
        json!({"grip": self.grip(), self.component_key(): self.component()})
    }
}

pub fn is_modular(spec: &WeaponSpec) -> bool { spec.kitgun.is_some() || spec.zaw.is_some() }

pub fn default_for(spec: &WeaponSpec) -> Option<Assembly> {
    if let Some(record) = &spec.kitgun {
        kitguns::default_assembly(record).map(Into::into)
    } else {
        zaws::default_assembly(spec).map(Into::into)
    }
}

/// Parts are read without repair for board validation, including incomplete pairs.
pub fn of(weapon: &str, grip: &str, loader: &str, link: &str) -> Option<Assembly> {
    if grip.is_empty() && loader.is_empty() && link.is_empty() { return None; }
    let zaw = super::spec(weapon).is_some_and(|s| s.zaw.is_some());
    if (zaw && !loader.is_empty()) || (!zaw && !link.is_empty()) {
        return Some(kitguns::Assembly { chamber: String::new(), grip: grip.into(), loader: loader.into() }.into());
    }
    if let Some(spec) = super::spec(weapon).filter(|s| s.zaw.is_some()) {
        Some(zaws::Assembly { record: spec.group().to_string(), grip: grip.into(), link: link.into() }.into())
    } else {
        kitguns::assembly_of(weapon, grip, loader).map(Into::into)
    }
}

/// Repair only invalid parts; a valid partner survives a stale share link.
pub fn repaired(spec: &WeaponSpec, grip: &str, loader: &str, link: &str) -> Option<Assembly> {
    let mut fallback = default_for(spec)?;
    match &mut fallback {
        Assembly::Kitgun(a) => {
            let slot = a.slot()?;
            if kitguns::grip(grip).is_some_and(|g| g.slot == slot) { a.grip = grip.into(); }
            if kitguns::loader(loader).is_some() { a.loader = loader.into(); }
        }
        Assembly::Zaw(a) => {
            let reference = spec.zaw.as_ref()?;
            if zaws::grips().iter().any(|g| g.id == grip && g.handedness == reference.handedness) { a.grip = grip.into(); }
            if zaws::links().iter().any(|l| l.id == link) { a.link = link.into(); }
        }
    }
    Some(fallback)
}
