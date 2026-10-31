//! Strike, handedness and parts resolve before the shared melee panel.
use super::WeaponSpec;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
pub struct Reference {
    pub strike: String,
    pub handedness: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Strike {
    pub id: String,
    pub name: String,
    pub damage: BTreeMap<String, f64>,
    pub speed_delta: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
    pub equal_damage_delta: bool,
    pub base_multiplier: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Grip {
    pub id: String,
    pub name: String,
    pub handedness: String,
    pub damage_delta: f64,
    pub base_speed: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Link {
    pub id: String,
    pub name: String,
    pub damage_delta: f64,
    pub speed_delta: f64,
    pub crit_chance_delta: f64,
    pub status_chance_delta: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assembly {
    pub record: String,
    pub grip: String,
    pub link: String,
}

#[derive(Debug, Deserialize)]
struct Parts {
    strikes: Vec<Strike>,
    grips: Vec<Grip>,
    links: Vec<Link>,
    defaults: BTreeMap<String, DefaultParts>,
}
#[derive(Debug, Deserialize)]
struct DefaultParts { grip: String, link: String }

fn parts() -> &'static Parts {
    static PARTS: OnceLock<Parts> = OnceLock::new();
    PARTS.get_or_init(|| {
        let (_, text) = crate::data::files_under("zaws/")
            .find(|(p, _)| *p == "zaws/parts.yaml").expect("Zaw parts");
        serde_norway::from_str(text).expect("valid Zaw parts")
    })
}
pub fn strikes() -> &'static [Strike] { &parts().strikes }
pub fn grips() -> &'static [Grip] { &parts().grips }
pub fn links() -> &'static [Link] { &parts().links }

pub fn default_assembly(spec: &WeaponSpec) -> Option<Assembly> {
    let reference = spec.zaw.as_ref()?;
    let pair = parts().defaults.get(&reference.handedness)?;
    Some(Assembly { record: spec.group().into(), grip: pair.grip.clone(), link: pair.link.clone() })
}

pub struct Stats {
    pub damage: BTreeMap<String, f64>,
    pub attack_speed: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
}

/// Component deltas precede the handedness multiplier; Plague types share the absolute delta.
pub fn stats(reference: &Reference, grip: &str, link: &str) -> Option<Stats> {
    let strike = strikes().iter().find(|s| s.id == reference.strike)?;
    let grip = grips().iter().find(|g| g.id == grip && g.handedness == reference.handedness)?;
    let link = links().iter().find(|l| l.id == link)?;
    let base = strike.damage.values().sum::<f64>();
    let total = (base + grip.damage_delta + link.damage_delta) * strike.base_multiplier.get(&reference.handedness)?;
    let damage = strike.damage.iter().map(|(kind, value)| {
        (kind.clone(), if strike.equal_damage_delta {
            value + (total - base) / strike.damage.len() as f64
        } else { value * total / base })
    }).collect();
    Some(Stats { damage, attack_speed: grip.base_speed + strike.speed_delta + link.speed_delta,
        crit_chance: strike.crit_chance + link.crit_chance_delta,
        crit_multiplier: strike.crit_multiplier, status_chance: strike.status_chance + link.status_chance_delta })
}

pub fn compose(spec: &WeaponSpec, assembly: &Assembly) -> Option<WeaponSpec> {
    if assembly.record != spec.group() { return None; }
    let built = stats(spec.zaw.as_ref()?, &assembly.grip, &assembly.link)?;
    let base = super::spec(spec.group())?;
    let ratio = built.damage.values().sum::<f64>() / base.attack.damage.values().sum::<f64>();
    let mut out = spec.clone();
    if out.attack.slam.is_none() { out.attack.slam = base.attack.slam.clone(); }
    for radial in [&mut out.attack.radial, &mut out.attack.slam].into_iter().flatten() {
        for value in radial.damage.values_mut() { *value *= ratio; }
    }
    if out.form != "heavy_slam" { out.attack.damage = built.damage; }
    out.attack.fire_rate = built.attack_speed;
    out.attack.crit_chance = built.crit_chance;
    out.attack.crit_multiplier = built.crit_multiplier;
    out.attack.status_chance = built.status_chance;
    Some(out)
}
