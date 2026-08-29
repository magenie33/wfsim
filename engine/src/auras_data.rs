//! AURAS — the SQUAD's mod, and the first thing in this engine that is not the
//! weapon's, not the build's, and not the target's.
//!
//! An aura sits on the WARFRAME, so it is read into the fight's [`Tenno`] beside
//! armour and energy — the block that becomes a real frame when frames are
//! built. That placement is the whole design: everything on
//! the Tenno already travels through `parse_fight` into the simulator AND the
//! optimizer, so neither module learns that auras exist.
//!
//! IT IS THE FIGHT'S, NEVER THE BUILD'S. Two players with the same gun and
//! different squads are two different fights, which is the same rule
//! `data/abilities/` follows and for the same reason — so an aura can no more
//! reach the BOARD than Roar can.
//!
//! [`Tenno`]: crate::tenno_data::Tenno

use serde::Deserialize;

/// What an aura DOES, typed — an unknown kind is refused rather than ignored.
///
/// The lesson is `arcanes_data::arc_condition`'s: a data file stating a rule the
/// engine does not apply is worse than one that omits it, because to anyone
/// auditing it reads as if the rule were being applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AuraEffect {
    /// Corrosive Projection. A MULTIPLIER on the target's armour, negative.
    EnemyArmor(f64),
    /// Shield Disruption, EMP Aura. The same shape on shields.
    EnemyShield(f64),
    /// The Amp family and Dead Eye — a base-damage bonus for ONE weapon class,
    /// which is why `requires` is not optional on them.
    WeaponDamage(f64),
    /// Coaction Drift: it multiplies the OTHER auras and does nothing itself.
    AuraStrength(f64),
}

#[derive(Debug, Clone, Deserialize)]
struct RawEffect {
    kind: String,
    #[serde(rename = "rankMax")]
    rank_max: f64,
    #[serde(default)]
    requires_pool: Option<String>,
    #[serde(default)]
    requires_class: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAura {
    id: String,
    name: String,
    #[serde(default)]
    squad_stacking: bool,
    effects: Vec<RawEffect>,
}

/// One aura the roster knows about.
#[derive(Debug, Clone)]
pub struct AuraDef {
    pub id: String,
    pub name: String,
    /// Does running it four-handed multiply it? Corrosive Projection's page:
    /// *"reducing enemy armor up to 72% with a 4-player squad"*.
    pub squad_stacking: bool,
    pub effect: AuraEffect,
    /// WHAT IT PAYS, and the family does not agree on the question. Three of
    /// the four amps match a MOD POOL — Rifle Amp "also affects bows, sniper
    /// rifles and launchers" and not shotguns, which is the `rifle` pool
    /// exactly. Dead Eye matches a CLASS and is narrower than any pool:
    /// *"only affects actual sniper rifles … even though bows and launchers
    /// draw from the sniper ammo pool, they are not affected"*.
    pub requires_pool: Option<String>,
    pub requires_class: Option<String>,
}

impl AuraDef {
    /// Does this aura pay a weapon of this class, drawing these pools?
    pub fn pays(&self, class: &str, pools: &[&str]) -> bool {
        match (&self.requires_pool, &self.requires_class) {
            (None, None) => true,
            (Some(p), _) if pools.contains(&p.as_str()) => true,
            (_, Some(c)) if c == class => true,
            _ => false,
        }
    }
}

/// THE PICK: which aura, at what rank, and how many of the squad run it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AuraPick {
    pub id: String,
    /// 1-4. A squad-stacking aura is additive with itself; one that is not
    /// ignores this.
    #[serde(default = "one")]
    pub count: u32,
}

fn one() -> u32 {
    1
}

fn parse_effect(e: &RawEffect) -> AuraEffect {
    match e.kind.as_str() {
        "enemy_armor_multiplier" => AuraEffect::EnemyArmor(e.rank_max),
        "enemy_shield_multiplier" => AuraEffect::EnemyShield(e.rank_max),
        "weapon_damage_bonus" => AuraEffect::WeaponDamage(e.rank_max),
        "aura_strength_bonus" => AuraEffect::AuraStrength(e.rank_max),
        other => panic!("unknown aura effect kind: {other}"),
    }
}

/// Every aura in `data/auras/`, loaded once.
pub fn all() -> &'static [AuraDef] {
    use std::sync::OnceLock;
    static A: OnceLock<Vec<AuraDef>> = OnceLock::new();
    A.get_or_init(|| {
        crate::data::files_under("auras/")
            .map(|(p, text)| {
                let r: RawAura = serde_norway::from_str(text)
                    .unwrap_or_else(|e| panic!("{p}: {e}"));
                let e = r.effects.first().unwrap_or_else(|| panic!("{p}: no effect"));
                AuraDef {
                    id: r.id,
                    name: r.name,
                    squad_stacking: r.squad_stacking,
                    effect: parse_effect(e),
                    requires_pool: e.requires_pool.clone(),
                    requires_class: e.requires_class.clone(),
                }
            })
            .collect()
    })
}

pub fn by_id(id: &str) -> Option<&'static AuraDef> {
    all().iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EVERY AURA LOADS AND ITS KIND IS ONE THIS ENGINE KNOWS.
    ///
    /// Derived from the directory rather than from a list of names, which is the
    /// rule a hand list keeps teaching: a hand list cannot report what is not
    /// on it.
    #[test]
    fn every_aura_parses_into_a_kind_this_engine_applies() {
        let a = all();
        assert!(a.len() >= 8, "the roster loaded: {}", a.len());
        // …and the one the whole design was written around is there, with the
        // number the wiki prints and the squad rule it states.
        let cp = by_id("corrosive_projection").expect("Corrosive Projection");
        assert_eq!(cp.effect, AuraEffect::EnemyArmor(-0.18));
        assert!(cp.squad_stacking, "it stacks to 72% four-handed");
        // A GATED ONE NAMES ITS GATE. An amp with neither would pay every
        // weapon, which is the one way this family can be wrong.
        //
        // …AND THE TWO GATES ARE DIFFERENT QUESTIONS. Dead Eye is the control:
        // it is the only amp that names a CLASS, because it is narrower than
        // any pool — bows draw sniper ammo and are not paid.
        assert_eq!(by_id("dead_eye").unwrap().requires_class.as_deref(), Some("sniper"));
        assert_eq!(by_id("rifle_amp").unwrap().requires_pool.as_deref(), Some("rifle"));
        for id in ["rifle_amp", "pistol_amp", "shotgun_amp", "dead_eye"] {
            let d = by_id(id).unwrap_or_else(|| panic!("{id}"));
            assert!(
                d.requires_pool.is_some() || d.requires_class.is_some(),
                "{id} must name what it pays — an amp with neither pays everything"
            );
            assert!(matches!(d.effect, AuraEffect::WeaponDamage(_)));
        }
    }
}
