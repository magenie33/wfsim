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
    /// NOTHING A FIGHT READS: every effect is `out_of_scope`. The Warframe
    /// builder still seats it and says why; the fight's roster leaves it out.
    None,
}

#[derive(Debug, Clone, Deserialize)]
struct RawEffect {
    kind: String,
    #[serde(rename = "rankMax", default)]
    rank_max: f64,
    #[serde(default)]
    requires_pool: Option<String>,
    #[serde(default)]
    requires_class: Option<String>,
    #[serde(default)]
    applies_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAura {
    id: String,
    name: String,
    polarity: String,
    base_drain: u32,
    max_rank: u32,
    #[serde(default)]
    rarity: Option<String>,
    #[serde(default)]
    exilus: bool,
    #[serde(default)]
    internal_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    squad_stacking: bool,
    effects: Vec<RawEffect>,
}

/// One aura the roster knows about.
#[derive(Debug, Clone)]
pub struct AuraDef {
    pub id: String,
    pub name: String,
    /// The card, as the Warframe builder seats it. `base_drain` is the RANK-0
    /// capacity the aura GRANTS, the same rank-0 convention a mod's cost uses.
    pub polarity: String,
    pub base_drain: u32,
    pub max_rank: u32,
    pub rarity: String,
    /// Coaction Drift is filed here because the fight reads it as an aura, and
    /// it is seated in the EXILUS slot like any other exilus mod.
    pub exilus: bool,
    pub internal_name: Option<String>,
    pub description: String,
    /// Each `out_of_scope` effect's own reason, in card order.
    pub out_of_scope: Vec<String>,
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
        if self.effect == AuraEffect::None {
            return false;
        }
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
        "out_of_scope" => AuraEffect::None,
        other => panic!("unknown aura effect kind: {other}"),
    }
}

/// Is this aura one the FIGHT can read? The squad picker lists only these.
pub fn in_fight(a: &AuraDef) -> bool {
    a.effect != AuraEffect::None
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
                assert!(!r.effects.is_empty(), "{p}: no effect");
                // THE FIRST EFFECT A FIGHT READS, or none when every one is out
                // of scope. Every kind is still parsed, so a typo panics here.
                let effects: Vec<AuraEffect> = r.effects.iter().map(parse_effect).collect();
                let at = effects.iter().position(|e| *e != AuraEffect::None);
                let e = &r.effects[at.unwrap_or(0)];
                AuraDef {
                    id: r.id,
                    name: r.name,
                    polarity: r.polarity,
                    base_drain: r.base_drain,
                    max_rank: r.max_rank,
                    rarity: r.rarity.unwrap_or_else(|| "common".into()),
                    exilus: r.exilus,
                    internal_name: r.internal_name,
                    description: r.description.unwrap_or_default(),
                    out_of_scope: r
                        .effects
                        .iter()
                        .filter(|x| x.kind == "out_of_scope")
                        .map(|x| x.applies_to.clone().unwrap_or_default())
                        .collect(),
                    squad_stacking: r.squad_stacking,
                    effect: at.map(|i| effects[i]).unwrap_or(AuraEffect::None),
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
